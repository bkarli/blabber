use std::{fmt::format, sync::Arc, time::{SystemTime, UNIX_EPOCH}};

use anyhow::Result;
use iroh_blobs::store::fs::FsStore;
use iroh_docs::{AuthorId, DocTicket, api::Doc, engine::LiveEvent, protocol::Docs, store::Query};
use n0_future::StreamExt;
use tokio::{sync::Mutex, task::JoinHandle};
use uuid::Uuid;
use serde::{Serialize,Deserialize};

use crate::Node;

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
    pub cache: Arc<Mutex<Vec<Message>>>
}

impl Room {
    /// Create a new room
    pub async fn new(docs: &Docs, name: impl Into<String>) -> Result<Self> {
        let messages = docs.create().await?;

        Ok(Self {
            id: Uuid::new_v4(),
            name: name.into(),
            messages,
            cache: Arc::new(Mutex::new(Vec::new()))
        })
    }
    
    /// Construct the Room from the ticket
    pub async fn from_ticket(docs: &Docs,id: Uuid, name: impl Into<String>, ticket: DocTicket) -> Result<Self> {
        let messages = docs.import(ticket).await?;

        Ok(Self {
            id: id,
            name: name.into(),
            messages,
            cache: Arc::new(Mutex::new(Vec::new()))
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
        
        let key = format!("msg/{sent_at:020}-{author}");

        let value = postcard::to_allocvec(&message)?;
        self.messages.set_bytes(author, key.into_bytes(), value).await?;

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
            let bytes = blobs.blobs().get_bytes(entry.content_hash()).await?;
            let message: Message = postcard::from_bytes(&bytes)?;
            messages.push(message);
        }
        Ok(messages)
    }

    /// If a new message event comes in apply the message to the in memory
    /// cache
    async fn apply_event(cache: Arc<Mutex<Vec<Message>>>,event: LiveEvent, blobs: &FsStore) {
        if let LiveEvent::InsertRemote { entry, .. } | LiveEvent::InsertLocal { entry, .. } = event {
            if let Ok(bytes) = blobs.blobs().get_bytes(entry.content_hash()).await {
                if let Ok(message) = postcard::from_bytes::<Message>(&bytes) {
                    cache.lock().await.push(message);
                }
            }
        }
    }
    
    // watch the document for that room and listen for updates
    pub async fn watch(&self, node: &Node, blobs: FsStore, label: impl Into<String>) -> Result<JoinHandle<()>> {
        let existing = self.list_messages(blobs.clone()).await?;
        *self.cache.lock().await = existing;

        let doc = self.messages.clone();
        let cache = self.cache.clone();
        let label = label.into();
        
        // the watch the doc
        let handle = node.watch_doc(doc, label, move |event| {
            let cache = cache.clone();
            let blobs = blobs.clone();

            async move {
                Room::apply_event(cache, event, &blobs).await;
            }
        });
        Ok(handle)
    }
}
