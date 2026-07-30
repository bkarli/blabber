use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use iroh_blobs::{store::fs::FsStore, Hash};
use iroh_docs::api::protocol::{AddrInfoOptions, ShareMode};
use iroh_docs::api::Doc;
use iroh_docs::protocol::Docs;
use iroh_docs::{AuthorId, DocTicket, Entry, engine::LiveEvent, store::Query};
use n0_future::StreamExt;
use rand::prelude::*;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, Mutex};
use tokio::task::JoinHandle;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::call_rooms::CallRoom;
use crate::invite::{Invite, RelayInvite};
use crate::room::Room;
use crate::{events, crypto, AppEvent, Node};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Member {
    pub author_id: String,
    pub endpoint_id: String,
    pub display_name: String,
    pub joined_at: u64,
    /// True for a blind relays own presence entry.
    #[serde(default)]
    pub is_relay: bool,
}

#[derive(Serialize, Deserialize)]
pub struct RoomRecord {
    id: Uuid,
    name: String,
    messages_ticket: String,
    media_ticket: String,
}

/// Cleartext discovery pointer for a room: no name, and both tickets are read only, a blind relay
/// can sync/seed both the messages and media docs without being able
/// to write to them or decrypt their content.
#[derive(Serialize, Deserialize)]
pub struct RoomRelayRecord {
    id: Uuid,
    messages_ticket: String,
    media_ticket: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct CallRoomRecord {
    id: Uuid,
    name: String,
    ticket: String,
}

/// Cleartext discovery pointer for a call room: read only ticket, no name.
#[derive(Serialize, Deserialize, Debug, Clone)]
struct CallRoomRelayRecord {
    id: Uuid,
    ticket: String,
}

enum SpaceEvents {
    RecordEvent,
    MemberEvent,
}

/// A shared space: an `info` doc and a `members`
/// doc, synced peer-to-peer through iroh-docs. Every entry is
/// encrypted with `key` except the `blind relay` discovery pointers
/// (`room-relay/`, `callroom-relay/`, `relay/`), which are
/// cleartext so a keyless relay can still find and distribute them. `key` and
/// `author` are `None` for a blind relay: it can sync and seed every doc,
/// but never decrypt or write real content.
#[derive(Clone)]
pub struct Space {
    id: uuid::Uuid,
    name: String,

    pub info: Doc,
    pub members: Doc,
    /// `None` for a blind relay
    key: Option<Arc<Zeroizing<[u8; 32]>>>,
    /// `None` for a blind relay
    author: Option<AuthorId>,
    pub rooms: Arc<Mutex<Vec<Room>>>,
    pub docs: Docs,
    pub call_rooms: Arc<Mutex<Vec<CallRoom>>>,

    pub member_cache: Arc<Mutex<Vec<Member>>>,
    pending_member: Arc<Mutex<HashMap<Hash, Entry>>>,
    pending_room: Arc<Mutex<HashMap<Hash, Entry>>>,

    /// background tasks spawned by `watch_members`/`watch_info`, kept
    /// so `leave` can stop them instead of leaking them detached.
    watch_handles: Arc<Mutex<Vec<JoinHandle<()>>>>,
}

impl Space {
    /// Creates a new space with a freshly generated key. `id` comes
    /// from the caller rather than being generated here, since
    /// `Node::create_space` needs it up front to derive `author` before it
    /// can build this `Space` at all.
    pub async fn new(
        docs: &Docs,
        id: Uuid,
        author: AuthorId,
        endpoint_id: String,
        display_name: String,
        name: impl Into<String>
        ) -> Result<Self> {
        let info = docs.create().await?;
        let members = docs.create().await?;

        let key = Some(Arc::new(Zeroizing::new(rand::rng().random::<[u8; 32]>())));

        let space = Self {
            id,
            name: name.into(),
            info,
            members,
            key,
            author: Some(author),
            rooms: Arc::new(Mutex::new(Vec::new())),
            docs: docs.clone(),
            call_rooms: Arc::new(Mutex::new(Vec::new())),

            member_cache: Arc::new(Mutex::new(Vec::new())),
            pending_member: Arc::new(Mutex::new(HashMap::new())),
            pending_room: Arc::new(Mutex::new(HashMap::new())),
            watch_handles: Arc::new(Mutex::new(Vec::new())),
        };

        space
            .insert_self_as_member(author, endpoint_id, display_name)
            .await?;

        Ok(space)
    }

    /// Once invited or space created insert yourself as a member
    async fn insert_self_as_member(
        &self,
        author: AuthorId,
        endpoint_id: String,
        display_name: String,
    ) -> Result<()> {
        let space_key = self
            .key
            .as_ref()
            .context("cannot write member record without space key (blind relay?)")?;

        let joined_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let member = Member {
            author_id: author.to_string(),
            endpoint_id: endpoint_id.to_string(),
            display_name: display_name.to_string(),
            joined_at,
            is_relay: false,
        };

        let key = format!("member/{author}");
        let plaintext = postcard::to_allocvec(&member)?;
        let value = crypto::encrypt(space_key, &plaintext, self.id.as_bytes())?;

        self.members.set_bytes(author, key.into_bytes(), value).await?;
        Ok(())
    }

    /// Announce this relay's own presence in the members doc, so a
    /// real member's member list shows "there's a relay attached to this
    /// space"
    async fn insert_self_as_relay(
        &self,
        author: AuthorId,
        endpoint_id: String,
        display_name: String,
    ) -> Result<()> {
        let joined_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let member = Member {
            author_id: author.to_string(),
            endpoint_id,
            display_name,
            joined_at,
            is_relay: true,
        };

        let key = format!("relay/{author}");
        let plaintext = postcard::to_allocvec(&member)?;
        self.members.set_bytes(author, key.into_bytes(), plaintext).await?;
        Ok(())
    }

    /// Remove yourself from the members doc. This writes a "tombstone" entry
    /// empty value under same key, rather than deleting anything
    /// locally-only, so the departure distributes to every other peer that's
    /// syncing this space's `members` doc.
    async fn remove_self_as_member(&self, author: AuthorId) -> Result<()> {
        let key = format!("member/{author}");
        self.members.del(author, key.into_bytes()).await?;
        Ok(())
    }

    /// Leave this space definitively: announce the departure to other members by writing tombstone,
    /// stop syncing, and drop the local document replicas.
    ///
    /// Note: the tombstone write above is only queued for replication when
    /// this returns, not confirmed by every peer. We give it a
    /// short window to go out over any already-connected gossip/sync
    /// sessions before tearing the local docs down. peers we weren't
    /// connected to at the moment of leaving may not see the
    /// departure until they next sync with someone else who did.
    pub async fn leave(&self, author: AuthorId) -> Result<()> {
        self.remove_self_as_member(author).await?;

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        for handle in self.watch_handles.lock().await.drain(..) {
            handle.abort();
        }

        self.docs.drop_doc(self.members.id()).await.ok();
        self.docs.drop_doc(self.info.id()).await.ok();

        Ok(())
    }

    pub async fn from_invite(
        docs: &Docs,
        invite: Invite,
        author: AuthorId,
        endpoint_id: String,
        display_name: impl Into<String>,
    ) -> Result<Self> {
        let info_ticket = DocTicket::from_str(&invite.info_ticket)?;
        let member_ticket = DocTicket::from_str(&invite.member_ticket)?;

        let info = docs.import(info_ticket).await?;
        let members = docs.import(member_ticket).await?;
        let display_name = display_name.into();

        let space = Self {
            id: invite.space_id,
            name: invite.space_name.clone(),
            info,
            members,
            key: Some(Arc::new(Zeroizing::new(invite.space_key))),
            author: Some(author),
            rooms: Arc::new(Mutex::new(Vec::new())),
            docs: docs.clone(),
            call_rooms: Arc::new(Mutex::new(Vec::new())),

            member_cache: Arc::new(Mutex::new(Vec::new())),
            pending_member: Arc::new(Mutex::new(HashMap::new())),
            pending_room: Arc::new(Mutex::new(HashMap::new())),
            watch_handles: Arc::new(Mutex::new(Vec::new())),
        };

        space
            .insert_self_as_member(author, endpoint_id, display_name)
            .await?;

        Ok(space)
    }

    /// Create a space from a RelayInvite: a keyless blind relay. It can
    /// sync/seed the space's docs but never decrypt or write real content.
    /// It does get its own author, used for one
    /// thing: signing its own cleartext presence entry in the members doc.
    pub async fn from_relay_invite(
        docs: &Docs,
        invite: RelayInvite,
        author: AuthorId,
        endpoint_id: String,
        display_name: impl Into<String>,
    ) -> Result<Self> {
        let info_ticket = DocTicket::from_str(&invite.info_ticket)?;
        let member_ticket = DocTicket::from_str(&invite.member_ticket)?;

        let info = docs.import(info_ticket).await?;
        let members = docs.import(member_ticket).await?;
        let display_name = display_name.into();

        let space = Self {
            id: invite.space_id,
            name: invite.space_name.clone(),
            info,
            members,
            key: None,
            author: Some(author),
            rooms: Arc::new(Mutex::new(Vec::new())),
            docs: docs.clone(),
            call_rooms: Arc::new(Mutex::new(Vec::new())),

            member_cache: Arc::new(Mutex::new(Vec::new())),
            pending_member: Arc::new(Mutex::new(HashMap::new())),
            pending_room: Arc::new(Mutex::new(HashMap::new())),
            watch_handles: Arc::new(Mutex::new(Vec::new())),
        };

        space
            .insert_self_as_relay(author, endpoint_id, display_name)
            .await?;

        Ok(space)
    }

    pub async fn create_invite(&self) -> Result<Invite> {
        Invite::from_space(self).await
    }

    pub async fn create_relay_invite(&self) -> Result<RelayInvite> {
        RelayInvite::from_space(self).await
    }

    /// Persists this space's invite under
    /// `root_path/<space-id>/meta/invite.txt`, so `Node::load_spaces` can
    /// reload it later. Picks a real `Invite` or a keyless `RelayInvite`
    /// depending on whether this space has a key.
    pub async fn create_directory(&self, root_path: &std::path::Path, storage_key: &[u8; 32]) -> Result<()> {
        let meta_dir = root_path.join(self.id.to_string()).join("meta");
        tokio::fs::create_dir_all(&meta_dir).await?;
        let encrypted = if self.key.is_some() {
            self.create_invite().await?.serialize_invite_encrypted(storage_key)?
        } else {
            self.create_relay_invite().await?.serialize_invite_encrypted(storage_key)?
        };
        tokio::fs::write(meta_dir.join("invite.txt"), encrypted).await?;
        Ok(())
    }

    pub async fn list_members(&self, blobs: &FsStore) -> Result<Vec<Member>> {
        let mut members = Vec::new();

        // real, encrypted members, a blind relay has no key, so it never
        // learns these records.
        if let Some(space_key) = self.key.as_ref() {
            let space_id = self.id;
            members.extend(
                self.collect_member_entries(blobs, "member/", |bytes| {
                    let plaintext = crypto::decrypt(space_key, bytes, space_id.as_bytes()).ok()?;
                    postcard::from_bytes(&plaintext).ok()
                })
                .await?,
            );
        }

        // blind relay presence, cleartext, no space key needed to read , readable by anyone syncing this doc,
        // including another relay.
        members.extend(
            self.collect_member_entries(blobs, "relay/", |bytes| postcard::from_bytes(bytes).ok())
                .await?,
        );

        Ok(members)
    }

    /// Walks every latest entry under `prefix` in the members doc, decoding
    /// each with `decode`. A tombstone, a signer
    /// mismatch, a self-reported `author_id` mismatch, or a `decode` failure
    /// all skip that entry rather than failing the whole scan. doc
    /// entries are untrusted and often mid-sync.
    async fn collect_member_entries(
        &self,
        blobs: &FsStore,
        prefix: &str,
        decode: impl Fn(&[u8]) -> Option<Member>,
    ) -> Result<Vec<Member>> {
        let entries = self
            .members
            .get_many(Query::single_latest_per_key().key_prefix(prefix))
            .await?;
        let mut entries = std::pin::pin!(entries);
        let mut members = Vec::new();

        while let Some(entry) = entries.next().await {
            let entry = entry?;
            let key = std::str::from_utf8(entry.key())?;
            let claim_auth = key.strip_prefix(prefix).unwrap_or_default();
            if claim_auth != entry.author().to_string() {
                continue;
            }
            if entry.content_len() == 0 {
                continue; // tombstone: the author left, or the relay stopped announcing itself
            }
            let Some(bytes) = blobs.blobs().get_bytes(entry.content_hash()).await.ok() else {
                continue; // content may not have finished syncing yet
            };
            let Some(member) = decode(&bytes) else {
                continue;
            };
            if member.author_id != claim_auth {
                continue;
            }
            members.push(member);
        }
        Ok(members)
    }

    pub async fn create_room(&self, node: &Node, author: AuthorId, name: impl Into<String>) -> Result<Room> {
        let space_key = self
            .key
            .clone()
            .context("cannot create a room without space key (blind relay?)")?;

        let name = name.into();
        let room = Room::new(&self.docs, name.clone(), Some(space_key.clone())).await?;

        let messages_ticket = room.messages.share(ShareMode::Write, AddrInfoOptions::RelayAndAddresses).await?;
        let media_ticket = room.media.share(ShareMode::Write, AddrInfoOptions::RelayAndAddresses).await?;
        let messages_read_ticket = room.messages.share(ShareMode::Read, AddrInfoOptions::RelayAndAddresses).await?;
        let media_read_ticket = room.media.share(ShareMode::Read, AddrInfoOptions::RelayAndAddresses).await?;

        let record = RoomRecord {
            id: room.id,
            name: name.clone(),
            messages_ticket: messages_ticket.to_string(),
            media_ticket: media_ticket.to_string(),
        };
        let relay_record = RoomRelayRecord {
            id: room.id,
            messages_ticket: messages_read_ticket.to_string(),
            media_ticket: media_read_ticket.to_string(),
        };

        let key = format!("room/{}", room.id);
        let plaintext = postcard::to_allocvec(&record)?;
        let value = crypto::encrypt(&space_key, &plaintext, self.id.as_bytes())?;
        self.info.set_bytes(author, key.into_bytes(), value).await?;

        // cleartext pointer, readable without the space key
        let relay_key = format!("room-relay/{}", room.id);
        let relay_value = postcard::to_allocvec(&relay_record)?;
        self.info.set_bytes(author, relay_key.into_bytes(), relay_value).await?;

        self.rooms.lock().await.push(room.clone());

        let blobs = node.blobs.clone().context("blobs not created yet")?;

        let label = format!("{}/{}", self.name, room.name);
        room.watch(node.events.clone(), blobs, label, self.id).await?;

        let _ = node.events.send(events::AppEvent::NewRoom {
            space_id: self.id,
            room_id: room.id,
            room_name: room.name.clone(),
        });

        Ok(room)
    }

    pub async fn create_call_room(&self, author: AuthorId, name: impl Into<String>) -> Result<CallRoom> {
        let space_key = self
            .key
            .clone()
            .context("cannot create a call room without space key (blind relay?)")?;

        let name = name.into();
        let room = CallRoom::new(&self.docs, name.clone(), Some(space_key.clone())).await?;
        let write_ticket = room.call_log.share(ShareMode::Write, AddrInfoOptions::RelayAndAddresses).await?;
        let read_ticket = room.call_log.share(ShareMode::Read, AddrInfoOptions::RelayAndAddresses).await?;

        let record = CallRoomRecord {
            id: room.id,
            name: name.clone(),
            ticket: write_ticket.to_string(),
        };
        let relay_record = CallRoomRelayRecord {
            id: room.id,
            ticket: read_ticket.to_string(),
        };

        let key = format!("callroom/{}", room.id);
        let plaintext = postcard::to_allocvec(&record)?;
        let value = crypto::encrypt(&space_key, &plaintext, self.id.as_bytes())?;
        self.info.set_bytes(author, key.into_bytes(), value).await?;

        let relay_key = format!("callroom-relay/{}", room.id);
        let relay_value = postcard::to_allocvec(&relay_record)?;
        self.info.set_bytes(author, relay_key.into_bytes(), relay_value).await?;

        self.call_rooms.lock().await.push(room.clone());
        Ok(room)
    }

    async fn is_known_room(&self, id: Uuid) -> bool {
        self.rooms.lock().await.iter().any(|r| r.id == id)
    }

    async fn is_known_call_room(&self, id: Uuid) -> bool {
        self.call_rooms.lock().await.iter().any(|r| r.id == id)
    }

    /// Called on join/reload to catch up on rooms created before we started
    /// watching the info doc live.
    pub async fn sync_rooms(&self, node: &Node, blobs: &FsStore) -> Result<Vec<JoinHandle<()>>> {
        match self.key.clone() {
            Some(space_key) => {
                let entries = self
                    .info
                    .get_many(Query::single_latest_per_key().key_prefix("room/"))
                    .await?;

                let mut entries = std::pin::pin!(entries);

                while let Some(entry) = entries.next().await {
                    let entry = entry?;
                    // content may not have finished downloading yet if the entry metadata
                    // synced ahead of the blob content; skip it for now, a later sync will pick it up
                    let Some(bytes) = blobs.blobs().get_bytes(entry.content_hash()).await.ok() else {
                        continue;
                    };
                    let Ok(plaintext) = crypto::decrypt(&space_key, &bytes, self.id.as_bytes()) else {
                        continue;
                    };
                    let record: RoomRecord = postcard::from_bytes(&plaintext)?;

                    if !self.is_known_room(record.id).await {
                        let messages_ticket = DocTicket::from_str(&record.messages_ticket)?;
                        let media_ticket = DocTicket::from_str(&record.media_ticket)?;
                        let room = Room::from_ticket(&self.docs, record.id, record.name, messages_ticket, media_ticket, Some(space_key.clone())).await?;
                        self.rooms.lock().await.push(room);
                    }
                }
            }
            None => {
                // blind relay: no space key, discover rooms through the cleartext read only pointer instead
                let entries = self
                    .info
                    .get_many(Query::single_latest_per_key().key_prefix("room-relay/"))
                    .await?;

                let mut entries = std::pin::pin!(entries);

                while let Some(entry) = entries.next().await {
                    let entry = entry?;
                    let Some(bytes) = blobs.blobs().get_bytes(entry.content_hash()).await.ok() else {
                        continue;
                    };
                    let Ok(record) = postcard::from_bytes::<RoomRelayRecord>(&bytes) else {
                        continue;
                    };

                    if !self.is_known_room(record.id).await {
                        let messages_ticket = DocTicket::from_str(&record.messages_ticket)?;
                        let media_ticket = DocTicket::from_str(&record.media_ticket)?;
                        // relay never learns the real name; the id stands in
                        let room = Room::from_ticket(&self.docs, record.id, record.id.to_string(), messages_ticket, media_ticket, None).await?;
                        self.rooms.lock().await.push(room);
                    }
                }
            }
        }

        let rooms = self.rooms.lock().await;
        let mut handles = Vec::new();
        for room in rooms.iter() {
            let label = format!("{}/{}", self.name, room.name);
            handles.push(room.watch(node.events.clone(), blobs.clone(), label, self.id).await?);
        }
        Ok(handles)
    }

    /// Discover call rooms published in the space's info doc since we last checked,
    /// and track them locally. Membership within a room is read fresh on demand
    /// (via `CallRoom::list_active_members`), so no live subscription is needed here.
    pub async fn sync_call_rooms(&self, blobs: &FsStore) -> Result<()> {
        match self.key.clone() {
            Some(space_key) => {
                let entries = self
                    .info
                    .get_many(Query::single_latest_per_key().key_prefix("callroom/"))
                    .await?;
                let mut entries = std::pin::pin!(entries);
                while let Some(entry) = entries.next().await {
                    let entry = entry?;
                    // content may not have finished downloading yet if the entry metadata
                    // synced ahead of the blob content; skip it for now, a later sync will pick it up
                    let Some(bytes) = blobs.blobs().get_bytes(entry.content_hash()).await.ok() else {
                        continue;
                    };
                    let Ok(plaintext) = crypto::decrypt(&space_key, &bytes, self.id.as_bytes()) else {
                        continue;
                    };
                    let record: CallRoomRecord = postcard::from_bytes(&plaintext)?;
                    if !self.is_known_call_room(record.id).await {
                        let ticket = DocTicket::from_str(&record.ticket)?;
                        let room = CallRoom::from_ticket(&self.docs, record.id, record.name, ticket, Some(space_key.clone())).await?;
                        self.call_rooms.lock().await.push(room);
                    }
                }
            }
            None => {
                let entries = self
                    .info
                    .get_many(Query::single_latest_per_key().key_prefix("callroom-relay/"))
                    .await?;
                let mut entries = std::pin::pin!(entries);
                while let Some(entry) = entries.next().await {
                    let entry = entry?;
                    let Some(bytes) = blobs.blobs().get_bytes(entry.content_hash()).await.ok() else {
                        continue;
                    };
                    let Ok(record) = postcard::from_bytes::<CallRoomRelayRecord>(&bytes) else {
                        continue;
                    };
                    if !self.is_known_call_room(record.id).await {
                        let ticket = DocTicket::from_str(&record.ticket)?;
                        let room = CallRoom::from_ticket(&self.docs, record.id, record.id.to_string(), ticket, None).await?;
                        self.call_rooms.lock().await.push(room);
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn try_apply_entry(
        &self,
        event_type: SpaceEvents,
        entry: &Entry,
        blobs: &FsStore,
        events: &broadcast::Sender<AppEvent>,
        space_id: Uuid,
    ) -> bool {
        match event_type {
            SpaceEvents::MemberEvent => {
                let Ok(key) = std::str::from_utf8(entry.key()) else { return false };

                // a blind relay's own cleartext presence entry - no space
                // key needed to read it, so this branches ahead of the
                // encrypted member/ handling below.
                if let Some(claim_auth) = key.strip_prefix("relay/") {
                    if claim_auth != entry.author().to_string() {
                        return false;
                    }
                    let author_id = claim_auth.to_string();

                    if entry.content_len() == 0 {
                        self.member_cache.lock().await.retain(|m| m.author_id != author_id);
                        let _ = events.send(AppEvent::MemberLeft { space_id, author_id });
                        return true;
                    }

                    let Ok(bytes) = blobs.blobs().get_bytes(entry.content_hash()).await else { return false };
                    let Ok(member) = postcard::from_bytes::<Member>(&bytes) else { return false };
                    if member.author_id != author_id {
                        return false;
                    }

                    let mut cache = self.member_cache.lock().await;
                    match cache.iter_mut().find(|m| m.author_id == member.author_id) {
                        Some(existing) => *existing = member.clone(),
                        None => cache.push(member.clone()),
                    }
                    drop(cache);
                    let _ = events.send(AppEvent::NewMember { space_id, member });
                    return true;
                }

                let Some(claim_auth) = key.strip_prefix("member/") else { return false };
                if claim_auth != entry.author().to_string() {
                    return false;
                }
                let author_id = claim_auth.to_string();

                // `del` on the members doc writes a tombstone: an entry with
                // the same "member/{author}" key but empty content. Treat
                // as the author leaving rather than trying (and failing) to
                // decode it as a `Member`.
                if entry.content_len() == 0 {
                    self.member_cache.lock().await.retain(|m| m.author_id != author_id);
                    let _ = events.send(AppEvent::MemberLeft { space_id, author_id });
                    return true;
                }

                // no key: a blind relay can never decrypt this, so there's
                // nothing to retry later either - counts as handled.
                let Some(space_key) = self.key.as_ref() else {
                    return true;
                };

                let Ok(bytes) = blobs.blobs().get_bytes(entry.content_hash()).await else { return false };
                let Ok(plaintext) = crypto::decrypt(space_key, &bytes, self.id.as_bytes()) else { return false };
                let Ok(member) = postcard::from_bytes::<Member>(&plaintext) else { return false };
                // the payload's self-reported author_id must also match the signer
                if member.author_id != author_id {
                    return false;
                }

                let mut cache = self.member_cache.lock().await;
                match cache.iter_mut().find(|m| m.author_id == member.author_id) {
                    Some(existing) => *existing = member.clone(),
                    None => cache.push(member.clone()),
                }
                drop(cache);
                let _ = events.send(AppEvent::NewMember { space_id, member });
                true
            }
            SpaceEvents::RecordEvent => {
                let Ok(key) = std::str::from_utf8(entry.key()) else { return false };

                if key.starts_with("room/") {
                    let Some(space_key) = self.key.as_ref() else { return true };
                    let Ok(bytes) = blobs.blobs().get_bytes(entry.content_hash()).await else { return false };
                    let Ok(plaintext) = crypto::decrypt(space_key, &bytes, self.id.as_bytes()) else { return false };
                    let Ok(record) = postcard::from_bytes::<RoomRecord>(&plaintext) else { return false };

                    if !self.is_known_room(record.id).await {
                        if let Ok(messages_ticket) = DocTicket::from_str(&record.messages_ticket) {
                            let Ok(media_ticket) = DocTicket::from_str(&record.media_ticket) else {
                                return false;
                            };
                            if let Ok(room) = Room::from_ticket(&self.docs, record.id, record.name.clone(), messages_ticket, media_ticket, self.key.clone()).await {
                                self.rooms.lock().await.push(room.clone());
                                let label = format!("{}/{}", self.name, room.name);
                                match room.watch(events.clone(), blobs.clone(), label, space_id).await {
                                    Ok(handle) => self.watch_handles.lock().await.push(handle),
                                    Err(e) => eprintln!("failed to watch newly discovered room {}: {e:#}", room.id),
                                }
                                let _ = events.send(AppEvent::NewRoom {
                                    space_id,
                                    room_id: room.id,
                                    room_name: room.name.clone(),
                                });
                            }
                        }
                    }
                    return true;
                }

                if key.starts_with("room-relay/") {
                    // this cleartext pointer exists only so a blind relay can import the room read only
                    if self.key.is_some() {
                        return true;
                    }
                    let Ok(bytes) = blobs.blobs().get_bytes(entry.content_hash()).await else { return false };
                    let Ok(record) = postcard::from_bytes::<RoomRelayRecord>(&bytes) else { return false };

                    if !self.is_known_room(record.id).await {
                        if let Ok(messages_ticket) = DocTicket::from_str(&record.messages_ticket) {
                            let Ok(media_ticket) = DocTicket::from_str(&record.media_ticket) else {
                                return false;
                            };
                            if let Ok(room) = Room::from_ticket(&self.docs, record.id, record.id.to_string(), messages_ticket, media_ticket, None).await {
                                self.rooms.lock().await.push(room.clone());
                                let label = format!("{}/{}", self.name, room.name);
                                match room.watch(events.clone(), blobs.clone(), label, space_id).await {
                                    Ok(handle) => self.watch_handles.lock().await.push(handle),
                                    Err(e) => eprintln!("failed to watch newly discovered room {}: {e:#}", room.id),
                                }
                            }
                        }
                    }
                    return true;
                }

                if key.starts_with("callroom/") {
                    let Some(space_key) = self.key.as_ref() else { return true };
                    let Ok(bytes) = blobs.blobs().get_bytes(entry.content_hash()).await else { return false };
                    let Ok(plaintext) = crypto::decrypt(space_key, &bytes, self.id.as_bytes()) else { return false };
                    let Ok(record) = postcard::from_bytes::<CallRoomRecord>(&plaintext) else { return false };

                    if !self.is_known_call_room(record.id).await {
                        if let Ok(ticket) = DocTicket::from_str(&record.ticket) {
                            if let Ok(room) = CallRoom::from_ticket(&self.docs, record.id, record.name.clone(), ticket, self.key.clone()).await {
                                self.call_rooms.lock().await.push(room.clone());
                                let _ = events.send(AppEvent::NewCallRoom {
                                    space_id,
                                    room_id: room.id,
                                    room_name: room.name.clone(),
                                });
                            }
                        }
                    }
                    return true;
                }

                if key.starts_with("callroom-relay/") {
                    if self.key.is_some() {
                        return true;
                    }
                    let Ok(bytes) = blobs.blobs().get_bytes(entry.content_hash()).await else { return false };
                    let Ok(record) = postcard::from_bytes::<CallRoomRelayRecord>(&bytes) else { return false };

                    if !self.is_known_call_room(record.id).await {
                        if let Ok(ticket) = DocTicket::from_str(&record.ticket) {
                            if let Ok(room) = CallRoom::from_ticket(&self.docs, record.id, record.id.to_string(), ticket, None).await {
                                self.call_rooms.lock().await.push(room);
                            }
                        }
                    }
                    return true;
                }

                true
            }
        }
    }

    pub async fn apply_event(
        &self,
        event_type: SpaceEvents,
        pending: Arc<Mutex<HashMap<Hash, Entry>>>,
        event: LiveEvent,
        blobs: &FsStore,
        events: &broadcast::Sender<AppEvent>,
        space_id: Uuid,
    ) {
        match event {
            LiveEvent::InsertLocal { entry, .. } | LiveEvent::InsertRemote { entry, .. } => {
                let applied = self.try_apply_entry(event_type, &entry, blobs, events, space_id).await;
                if !applied {
                    pending.lock().await.insert(entry.content_hash(), entry);
                }
            }
            LiveEvent::ContentReady { hash } => {
                if let Some(entry) = pending.lock().await.remove(&hash) {
                    self.try_apply_entry(event_type, &entry, blobs, events, space_id).await;
                }
            }
            _ => {}
        }
    }

    pub async fn watch_members(
        &self,
        node: &Node,
        blobs: FsStore,
        label: impl Into<String>,
    ) -> Result<()> {
        let existing = self.list_members(&blobs).await?;
        *self.member_cache.lock().await = existing;

        let space = self.clone();
        let doc = self.members.clone();
        let pending = self.pending_member.clone();
        let label = label.into();
        let events = node.events.clone();
        let space_id = self.id();

        let handle = node.watch_doc(doc, label, move |event| {
            let space = space.clone();
            let pending = pending.clone();
            let blobs = blobs.clone();
            let events = events.clone();

            async move {
                space.apply_event(SpaceEvents::MemberEvent, pending, event, &blobs, &events, space_id).await;
            }
        }).await?;

        self.watch_handles.lock().await.push(handle);
        Ok(())
    }

    pub async fn watch_info(
        &self,
        node: &Node,
        blobs: FsStore,
        label: impl Into<String>,
    ) -> Result<()> {
        let doc = self.info.clone();
        let space = self.clone();
        let pending = self.pending_room.clone();
        let label = label.into();
        let events = node.events.clone();
        let space_id = self.id();

        let handle = node.watch_doc(doc, label, move |event| {
            let space = space.clone();
            let pending = pending.clone();
            let blobs = blobs.clone();
            let events = events.clone();

            async move {
                space.apply_event(SpaceEvents::RecordEvent, pending, event, &blobs, &events, space_id).await;
            }
        }).await?;

        self.watch_handles.lock().await.push(handle);
        Ok(())
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn key(&self) -> Option<Arc<Zeroizing<[u8; 32]>>> {
        self.key.clone()
    }

    pub fn author(&self) -> Option<AuthorId> {
        self.author
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}
