use std::{collections::HashMap, fmt::format, sync::Arc, time::{SystemTime, UNIX_EPOCH}};

use anyhow::Result;
use iroh_blobs::store::fs::FsStore;
use iroh_docs::{AuthorId, DocTicket, Entry, api::Doc, engine::LiveEvent, protocol::Docs, store::Query};
use n0_future::StreamExt;
use tokio::{sync::{Mutex, broadcast}, task::JoinHandle};
use uuid::Uuid;
use serde::{Serialize,Deserialize};

use crate::{Node, crypto, events::AppEvent};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
    pub author: String,
    pub content: String,
    pub sent_at: u64,
}

#[derive(Clone)]
pub struct Room {
    pub id: Uuid,
    pub name: String,
    pub messages: Doc,
    pub cache: Arc<Mutex<Vec<Message>>>,
    key: [u8; 32],

    pending: Arc<Mutex<HashMap<iroh_blobs::Hash, Entry>>>,
}

impl Room {
    /// Create a new room
    pub async fn new(docs: &Docs, name: impl Into<String>, key: [u8; 32]) -> Result<Self> {
        let messages = docs.create().await?;

        Ok(Self {
            id: Uuid::new_v4(),
            name: name.into(),
            messages,
            cache: Arc::new(Mutex::new(Vec::new())),
            key,
            pending: Arc::new(Mutex::new(HashMap::new()))
        })
    }

    /// Construct the Room from the ticket
    pub async fn from_ticket(docs: &Docs, id: Uuid, name: impl Into<String>, ticket: DocTicket, key: [u8; 32]) -> Result<Self> {
        let messages = docs.import(ticket).await?;

        Ok(Self {
            id: id,
            name: name.into(),
            messages,
            cache: Arc::new(Mutex::new(Vec::new())),
            key,
            pending: Arc::new(Mutex::new(HashMap::new()))
        })
    }

    pub async fn send_message(&self, author: AuthorId, content: impl Into<String>) -> Result<()> {
        let sent_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let message = Message {
            author: author.to_string(),
            content: content.into(),
            sent_at,

        };
        
        let doc_key = format!("msg/{sent_at:020}-{author}");

        let plaintext = postcard::to_allocvec(&message)?;
        let value = crypto::encrypt(&self.key, &plaintext, self.id.as_bytes())?;
        self.messages.set_bytes(author, doc_key.into_bytes(), value).await?;

        Ok(())
    }

    pub async fn list_messages(&self, blobs: FsStore) -> Result<Vec<Message>> {
        let entries = self
            .messages
            .get_many(Query::single_latest_per_key().key_prefix("msg/"))
            .await?;


        let mut entries = std::pin::pin!(entries);
        let mut messages = Vec::new();

        while let Some(entry) = entries.next().await {
            let entry = entry?;
            let Ok(bytes) = blobs.blobs().get_bytes(entry.content_hash()).await else { continue };
            let Ok(plaintext) = crypto::decrypt(&self.key, &bytes, self.id.as_bytes()) else { continue };
            let Ok(message) = postcard::from_bytes::<Message>(&plaintext) else { continue };
            messages.push(message);
        }
        Ok(messages)
    }

    /// If a new message event comes in apply the message to the in memory
    /// cache
    async fn try_apply_entry(
        cache: &Arc<Mutex<Vec<Message>>>,
        entry: &Entry,
        blobs: &FsStore,
        events: &broadcast::Sender<AppEvent>,
        space_id: Uuid,
        room_id: Uuid,
        key: [u8; 32],
    ) -> bool {
        if let Ok(bytes) = blobs.blobs().get_bytes(entry.content_hash()).await {
            if let Ok(plaintext) = crypto::decrypt(&key, &bytes, room_id.as_bytes()) {
                if let Ok(message) = postcard::from_bytes::<Message>(&plaintext) {
                    cache.lock().await.push(message.clone());
                    let a = events.send(AppEvent::NewMessage { space_id, room_id, message });
                    println!("{:?}", a);
                    return true;
                }
            }
        }
        false
    }

    async fn apply_event(
        cache: Arc<Mutex<Vec<Message>>>,
        pending: Arc<Mutex<HashMap<iroh_blobs::Hash, Entry>>>,
        event: LiveEvent,
        blobs: &FsStore,
        events: &broadcast::Sender<AppEvent>,
        space_id: Uuid,
        room_id: Uuid,
        key: [u8; 32],
    ) {
        match event {
            LiveEvent::InsertRemote { entry, .. } | LiveEvent::InsertLocal { entry, .. } => {
                let applied = Self::try_apply_entry(&cache, &entry, blobs, events, space_id, room_id, key).await;
                if !applied {
                    println!("not appliable");
                    pending.lock().await.insert(entry.content_hash(), entry);
                }
            }
            LiveEvent::ContentReady { hash } => {
                let stashed = pending.lock().await.remove(&hash);
                if let Some(entry) = stashed {
                    Self::try_apply_entry(&cache, &entry, blobs, events, space_id, room_id, key).await;
                    println!("Hash ready")
                }
            }
            _ => {}
        }
    }

    pub async fn watch(
        &self,
        node: &Node,
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
        let events = node.events.clone();
        let room_id = self.id;
        let key = self.key;

        let handle = node.watch_doc(doc, label, move |event| {
            let cache = cache.clone();
            let pending = pending.clone();
            let blobs = blobs.clone();
            let events = events.clone();

            async move {
                Room::apply_event(cache, pending, event, &blobs, &events, space_id, room_id, key).await;
            }
        }).await?;
        Ok(handle)
    }
}
