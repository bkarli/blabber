use std::{collections::HashMap, sync::Arc, time::{SystemTime, UNIX_EPOCH}};
use anyhow::Result;
use iroh_blobs::store::fs::FsStore;
use iroh_docs::{AuthorId, DocTicket, Entry, api::Doc, engine::LiveEvent, protocol::Docs, store::Query};
use n0_future::StreamExt;
use tokio::{sync::{Mutex, broadcast}, task::JoinHandle};
use uuid::Uuid;
use serde::{Serialize, Deserialize};
use crate::{Node, events::AppEvent};


#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CallLogEntry {
    pub participants: Vec<String>,
    pub started_at: u64,
    pub ended_at: Option<u64>,
}

#[derive(Clone)]
pub struct CallRoom {
    pub id: Uuid,
    pub name: String,
    pub participants: Arc<Mutex<Vec<String>>>,
    pub call_log: Doc,
    pub cache: Arc<Mutex<Vec<CallLogEntry>>>,
    pending: Arc<Mutex<HashMap<iroh_blobs::Hash, Entry>>>,
}

impl CallRoom{
    pub async fn new(docs: &Docs, name: impl Into<String>) -> Result<Self>{
        let call_log = docs.create().await?;

        Ok(Self{
            id: Uuid::new_v4(),
            name: name.into(),
            call_log,
            cache: Arc::new(Mutex::new(Vec::new())),
            participants: Arc::new(Mutex::new(Vec::new())),
            pending: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    //Construct VoiceCall Room from ticket
    pub async fn from_ticket(docs: &Docs, id: Uuid, name: impl Into<String>, ticket: DocTicket) -> Result<Self>{
        let call_log = docs.import(ticket).await?;
        Ok(Self{
            id,
            name: name.into(),
            call_log,
            cache: Arc::new(Mutex::new(Vec::new())),
            participants: Arc::new(Mutex::new(Vec::new())),
            pending: Arc::new(Mutex::new(HashMap::new())),
        })

    }

    pub async fn log_call_started(&self, author: AuthorId, participants: Vec<String>)->Result<()>{
        let started_at = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
        let entry = CallLogEntry{
            participants,
            started_at,
            ended_at: None,
        };
        let key = format!("call/{started_at:020}-{author}");
        let value = postcard::to_allocvec(&entry)?;
        self.call_log.set_bytes(author, key.into_bytes(), value).await?;
        Ok(())
    }

    pub async fn list_call_log(&self, blobs: FsStore)->Result<Vec<CallLogEntry>>{
        let entries = self.call_log.get_many(Query::single_latest_per_key().key_prefix("call/")).await?;
        let mut entries = std::pin::pin!(entries);
        let mut log = Vec::new();

        while let Some(entry) = entries.next().await{
            let entry = entry?;
            let bytes = blobs.blobs().get_bytes(entry.content_hash()).await?;
            let call_entry: CallLogEntry = postcard::from_bytes(&bytes)?;
            log.push(call_entry);
        }
        Ok(log)
    }
    async fn try_apply_entry(
        cache: &Arc<Mutex<Vec<CallLogEntry>>>,
        entry: &Entry,
        blobs: &FsStore,
        events: &broadcast::Sender<AppEvent>,
        space_id: Uuid,
        room_id: Uuid,
    ) -> bool {
        if let Ok(bytes) = blobs.blobs().get_bytes(entry.content_hash()).await {
            if let Ok(call_entry) = postcard::from_bytes::<CallLogEntry>(&bytes) {
                cache.lock().await.push(call_entry.clone());
                println!("Applying event");
                let _ = events.send(AppEvent::NewCallLogEntry { space_id, room_id, entry: call_entry });
                return true;
            }
        }
        false
    }

    async fn apply_event(
        cache: Arc<Mutex<Vec<CallLogEntry>>>,
        pending: Arc<Mutex<HashMap<iroh_blobs::Hash, Entry>>>,
        event: LiveEvent,
        blobs: &FsStore,
        events: &broadcast::Sender<AppEvent>,
        space_id: Uuid,
        room_id: Uuid,
    ) {
        match event {
            LiveEvent::InsertRemote { entry, .. } | LiveEvent::InsertLocal { entry, .. } => {
                let applied = Self::try_apply_entry(&cache, &entry, blobs, events, space_id, room_id).await;
                if !applied {
                    pending.lock().await.insert(entry.content_hash(), entry);
                }
            }
            LiveEvent::ContentReady { hash } => {
                let stashed = pending.lock().await.remove(&hash);
                if let Some(entry) = stashed {
                    Self::try_apply_entry(&cache, &entry, blobs, events, space_id, room_id).await;
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
        let existing = self.list_call_log(blobs.clone()).await?;
        *self.cache.lock().await = existing;

        let doc = self.call_log.clone();
        let cache = self.cache.clone();
        let pending = self.pending.clone();
        let label = label.into();
        let events = node.events.clone();
        let room_id = self.id;

        let handle = node.watch_doc(doc, label, move |event| {
            let cache = cache.clone();
            let pending = pending.clone();
            let blobs = blobs.clone();
            let events = events.clone();
            async move {
                Self::apply_event(cache, pending, event, &blobs, &events, space_id, room_id).await;
            }
        }).await?;
        Ok(handle)
    }
}
