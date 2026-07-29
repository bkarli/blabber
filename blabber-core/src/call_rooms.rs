use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use anyhow::{Context, Result};
use iroh_blobs::store::fs::FsStore;
use iroh_docs::{AuthorId, DocTicket, api::Doc, protocol::Docs, store::Query};
use n0_future::StreamExt;
use uuid::Uuid;
use serde::{Serialize, Deserialize};
use zeroize::Zeroizing;

use crate::crypto;

/// A member's current in/out-of-call state, as published to the room's shared doc.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CallMembership {
    pub endpoint_id: String,
    pub active: bool,
    pub updated_at: u64,
}

/// A mesh voice call room: its own synced doc tracks who's currently active,
/// so peers can discover and dial each other without a central server.
#[derive(Clone)]
pub struct CallRoom {
    pub id: Uuid,
    pub name: String,
    pub call_log: Doc,
    /// `None` for a blind relay
    key: Option<Arc<Zeroizing<[u8; 32]>>>,
}

impl CallRoom {
    /// Create a brand-new call room with a fresh doc.
    pub async fn new(docs: &Docs, name: impl Into<String>, key: Option<Arc<Zeroizing<[u8; 32]>>>) -> Result<Self> {
        let call_log = docs.create().await?;

        Ok(Self {
            id: Uuid::new_v4(),
            name: name.into(),
            call_log,
            key,
        })
    }

    /// Attach to a call room another peer created, via its shared doc ticket.
    pub async fn from_ticket(docs: &Docs, id: Uuid, name: impl Into<String>, ticket: DocTicket, key: Option<Arc<Zeroizing<[u8; 32]>>>) -> Result<Self> {
        let call_log = docs.import(ticket).await?;
        Ok(Self {
            id,
            name: name.into(),
            call_log,
            key,
        })
    }

    /// Publish this author's current in/out-of-call state for other peers to see.
    pub async fn set_membership(&self, author: AuthorId, endpoint_id: String, active: bool) -> Result<()> {
        let key = self
            .key
            .as_ref()
            .context("cannot publish call presence: no key (blind relay?)")?;

        let updated_at = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
        let membership = CallMembership { endpoint_id, active, updated_at };
        let doc_key = format!("member/{author}");
        let plaintext = postcard::to_allocvec(&membership)?;
        let value = crypto::encrypt(key, &plaintext, self.id.as_bytes())?;
        self.call_log.set_bytes(author, doc_key.into_bytes(), value).await?;
        Ok(())
    }

    /// List the endpoint ids of members currently marked active in this room.
    pub async fn list_active_members(&self, blobs: FsStore) -> Result<Vec<String>> {
        // a blind relay has no key, so it never learns whos in a call
        let Some(key) = self.key.as_ref() else {
            return Ok(Vec::new());
        };

        let entries = self.call_log.get_many(Query::single_latest_per_key().key_prefix("member/")).await?;
        let mut entries = std::pin::pin!(entries);
        let mut active_members = Vec::new();

        while let Some(entry) = entries.next().await {
            let entry = entry?;
            let Some(bytes) = blobs.blobs().get_bytes(entry.content_hash()).await.ok() else {
                continue;
            };
            let Ok(plaintext) = crypto::decrypt(key, &bytes, self.id.as_bytes()) else {
                continue;
            };
            let Ok(membership) = postcard::from_bytes::<CallMembership>(&plaintext) else {
                continue;
            };
            if membership.active {
                active_members.push(membership.endpoint_id);
            }
        }
        Ok(active_members)
    }
}
