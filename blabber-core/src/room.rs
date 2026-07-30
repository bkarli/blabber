use std::{collections::HashMap, sync::Arc, time::{SystemTime, UNIX_EPOCH}};
use base64::{engine::general_purpose::STANDARD as base64_engine, Engine as _};
use image::imageops::FilterType;

use anyhow::{Context, Result};
use iroh_blobs::store::fs::FsStore;
use iroh_docs::{AuthorId, DocTicket, Entry, api::Doc, engine::LiveEvent, protocol::Docs, store::Query};
use n0_future::StreamExt;
use tokio::{sync::{Mutex, broadcast}, task::JoinHandle};
use uuid::Uuid;
use serde::{Serialize, Deserialize};
use zeroize::Zeroizing;

use crate::{crypto, events::AppEvent};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
    pub author: String,
    pub content: MessageContent,
    pub sent_at: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum MessageContent {
    Text { text: String },
    Image {
        filename: String,
        mime: String,
        thumbnail_base64: String,
        media_key: String,
    },
    File {
        filename: String,
        mime: String,
        size: u64,
        media_key: String
    },
}

#[derive(Clone)]
pub struct Room {
    pub id: Uuid,
    pub name: String,
    pub messages: Doc,
    pub media: Doc,

    pub cache: Arc<Mutex<Vec<Message>>>,
    /// `None` for a blind relay
    key: Option<Arc<Zeroizing<[u8; 32]>>>,
    pending: Arc<Mutex<HashMap<iroh_blobs::Hash, Entry>>>,
}

impl Room {
    pub async fn new(
        docs: &Docs,
        name: impl Into<String>,
        key: Option<Arc<Zeroizing<[u8; 32]>>>
    )-> Result<Self> {
        let messages = docs.create().await?;
        let media = docs.create().await?;

        Ok(Self {
            id: Uuid::new_v4(),
            name: name.into(),
            messages,
            media,
            cache: Arc::new(Mutex::new(Vec::new())),
            key,
            pending: Arc::new(Mutex::new(HashMap::new()))
        })
    }

    /// Attach to a room another peer already created, via its shared doc tickets.
    pub async fn from_ticket(
        docs: &Docs,
        id: Uuid,
        name: impl Into<String>,
        ticket: DocTicket,
        media_ticket: DocTicket,
        key: Option<Arc<Zeroizing<[u8; 32]>>>
    ) -> Result<Self> {
        let messages = docs.import(ticket).await?;
        let media = docs.import(media_ticket).await?;
        Ok(Self {
            id,
            name: name.into(),
            messages,
            media,
            cache: Arc::new(Mutex::new(Vec::new())),
            key,
            pending: Arc::new(Mutex::new(HashMap::new()))
        })
    }

    pub async fn send_content(
        &self,
        author: AuthorId,
        content: MessageContent
    ) -> Result<()> {
        let key = self
            .key
            .as_ref()
            .context("cannot send a message: no decryption key (blind relay?)")?;

        let sent_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let message = Message {
            author: author.to_string(),
            content,
            sent_at
        };

        // opaque doc key: timestamp/author only live inside the encrypted payload
        let doc_key = format!("msg/{}", Uuid::new_v4());
        let plaintext = postcard::to_allocvec(&message)?;
        let value = crypto::encrypt(key, &plaintext, self.id.as_bytes())?;
        self.messages.set_bytes(author, doc_key.into_bytes(), value).await?;

        Ok(())
    }

    pub async fn send_message(&self, author: AuthorId, content: impl Into<String>) -> Result<()> {
        self.send_content(author, MessageContent::Text { text: content.into() }).await
    }

    fn generate_thumbnail(data: &[u8]) -> Result<String> {
        let img = image::load_from_memory(data)?;
        let thumb = img.resize(240, 240, FilterType::Triangle);

        let mut buf = Vec::new();
        thumb.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Jpeg)?;
        Ok(base64_engine.encode(&buf))
    }

    pub async fn store_media(
        &self,
        author: AuthorId,
        data: &[u8],
    ) -> Result<String> {
        let key = self
            .key
            .as_ref()
            .context("cannot store media: no encryption key (blind relay?)")?;

        let media_key = format!("media/{}", Uuid::new_v4());
        let encrypted = crypto::encrypt(key, data, self.id.as_bytes())?;

        self.media.set_bytes(author, media_key.clone().into_bytes(), encrypted).await?;
        Ok(media_key)
    }

    pub async fn send_image(
        &self,
        author: AuthorId,
        filename: impl Into<String>,
        mime: impl Into<String>,
        data: Vec<u8>,
    ) -> Result<()> {
        let thumbnail_base64 = Room::generate_thumbnail(&data)?;
        let media_key = self.store_media(author, &data).await?;

        self.send_content(author, MessageContent::Image {
            filename: filename.into(),
            mime: mime.into(),
            thumbnail_base64,
            media_key,
        }).await
    }
    
    pub async fn send_file(
        &self,
        author: AuthorId,
        filename: impl Into<String>,
        mime: impl Into<String>,
        data: Vec<u8>,
    ) -> Result<()> {
        let size = data.len() as u64;
        let media_key = self.store_media(author, &data).await?;

        self.send_content(author, MessageContent::File {
            filename: filename.into(),
            mime: mime.into(),
            size,
            media_key,
        }).await
    }

    /// `None` if the media isn't in the store yet (not synced) or this room has no key.
    pub async fn get_media(
        &self,
        media_key: &str,
        blobs: FsStore,
    ) -> Result<Option<Vec<u8>>> {
        let Some(key) = self.key.as_ref() else {
            return Ok(None);
        };

        let Some(entry) = self
            .media
            .get_one(Query::single_latest_per_key().key_exact(media_key))
            .await?
        else {
            return Ok(None);
        };

        let Ok(bytes) = blobs.blobs().get_bytes(entry.content_hash()).await else {
            return Ok(None);
        };

        let Ok(plaintext) = crypto::decrypt(key, &bytes, self.id.as_bytes()) else {
            return Ok(None);
        };

        Ok(Some(plaintext.to_vec()))
    }

    /// Decrypts and decodes a stored message, discarding it (returning
    /// `None`) rather than erroring on any corruption or key mismatch -
    /// callers iterate over untrusted/partially-synced doc entries and treat
    /// a bad one as simply absent.
    fn decode_message(key: &[u8; 32], aad: &[u8], ciphertext: &[u8]) -> Option<Message> {
        let plaintext = crypto::decrypt(key, ciphertext, aad).ok()?;
        postcard::from_bytes(&plaintext).ok()
    }

    pub async fn list_messages(&self, blobs: FsStore) -> Result<Vec<Message>> {
        // a blind relay has no key, so it never learns message content
        let Some(key) = self.key.as_ref() else {
            return Ok(Vec::new());
        };

        let entries = self
            .messages
            .get_many(Query::single_latest_per_key().key_prefix("msg/"))
            .await?;

        let mut entries = std::pin::pin!(entries);
        let mut messages = Vec::new();

        while let Some(entry) = entries.next().await {
            let entry = entry?;
            let Ok(bytes) = blobs.blobs().get_bytes(entry.content_hash()).await else { continue };
            let Some(message) = Self::decode_message(key, self.id.as_bytes(), &bytes) else { continue };
            messages.push(message);
        }
        Ok(messages)
    }

    pub async fn get_exact_message(
        &self,
        doc_key: impl Into<String>,
        blobs: FsStore,
    ) -> Result<Option<Message>> {
        let Some(key) = self.key.as_ref() else {
            return Ok(None);
        };

        let Some(entry) = self
            .messages
            .get_one(Query::single_latest_per_key().key_exact(doc_key.into()))
            .await?
        else {
            return Ok(None);
        };

        let Ok(bytes) = blobs.blobs().get_bytes(entry.content_hash()).await else {
            return Ok(None);
        };

        Ok(Self::decode_message(key, self.id.as_bytes(), &bytes))
    }

    async fn try_apply_entry(
        cache: &Arc<Mutex<Vec<Message>>>,
        entry: &Entry,
        blobs: &FsStore,
        events: &broadcast::Sender<AppEvent>,
        space_id: Uuid,
        room_id: Uuid,
        key: Option<Arc<Zeroizing<[u8; 32]>>>,
    ) -> bool {
        let Some(key) = key else { return false };
        let Ok(bytes) = blobs.blobs().get_bytes(entry.content_hash()).await else { return false };
        let Some(message) = Self::decode_message(&key, room_id.as_bytes(), &bytes) else { return false };

        cache.lock().await.push(message.clone());
        let _ = events.send(AppEvent::NewMessage { space_id, room_id, message });
        true
    }

    async fn apply_event(
        cache: Arc<Mutex<Vec<Message>>>,
        pending: Arc<Mutex<HashMap<iroh_blobs::Hash, Entry>>>,
        event: LiveEvent,
        blobs: &FsStore,
        events: &broadcast::Sender<AppEvent>,
        space_id: Uuid,
        room_id: Uuid,
        key: Option<Arc<Zeroizing<[u8; 32]>>>,
    ) {
        match event {
            LiveEvent::InsertRemote { entry, .. } | LiveEvent::InsertLocal { entry, .. } => {
                let applied = Self::try_apply_entry(&cache, &entry, blobs, events, space_id, room_id, key).await;
                if !applied {
                    pending.lock().await.insert(entry.content_hash(), entry);
                }
            }
            LiveEvent::ContentReady { hash } => {
                if let Some(entry) = pending.lock().await.remove(&hash) {
                    Self::try_apply_entry(&cache, &entry, blobs, events, space_id, room_id, key).await;
                }
            }
            _ => {}
        }
    }

    pub async fn watch(
        &self,
        events: broadcast::Sender<AppEvent>,
        blobs: FsStore,
        label: impl Into<String>,
        space_id: Uuid,
    ) -> Result<JoinHandle<()>> {
        let existing = self.list_messages(blobs.clone()).await?;
        *self.cache.lock().await = existing;

        let doc = self.messages.clone();
        let cache = self.cache.clone();
        let pending = self.pending.clone();
        let label = label.into();
        let room_id = self.id;
        let key = self.key.clone();

        let handle = crate::node::watch_doc(doc, label, move |event| {
            let cache = cache.clone();
            let pending = pending.clone();
            let blobs = blobs.clone();
            let events = events.clone();
            let key = key.clone();

            async move {
                Room::apply_event(
                    cache,
                    pending,
                    event,
                    &blobs,
                    &events,
                    space_id,
                    room_id,
                    key).await;
            }
        }).await?;
        Ok(handle)
    }
}
