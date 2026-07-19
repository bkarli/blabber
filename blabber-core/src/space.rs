use std::{collections::HashMap, str::FromStr, sync::Arc, task::Poll::Pending};
use anyhow::{Context};


use crate::{AppEvent, Node, events, invite::Invite};
use anyhow::{Result};
use iroh_blobs::{Hash, store::fs::FsStore};
use iroh_docs::{AuthorId, DocTicket, Entry, api::protocol::ShareMode, engine::LiveEvent, store::Query};
use n0_future::{Stream, StreamExt};
use tokio::{sync::{Mutex, broadcast}, task::JoinHandle};
use crate::room::Room;
use iroh_docs::api::Doc;
use iroh_docs::protocol::Docs;
use uuid::Uuid;
use serde::{Serialize, Deserialize};
use std::time::{SystemTime, UNIX_EPOCH};
use crate::call_rooms::CallRoom;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Member {
    pub author_id: String,
    pub endpoint_id: String,
    pub display_name: String,
    pub joined_at: u64,
}

#[derive(Serialize, Deserialize)]
pub struct RoomRecord {
    id: Uuid,
    name: String,
    ticket: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct CallRoomRecord {
    id: Uuid,
    name: String,
    ticket: String,
}

enum SpaceRecords {
    RoomRecord,
    CallRoomRecord,
}


enum SpaceEvents {
    RecordEvent,
    MemberEvent,
}

/// Space holds
/// - Each endpoint connected to it
/// - Rooms: Chat Rooms
/// - Channels Voice Rooms
///
/// You can invite other peers to that Space
/// They will see the same rooms and channels
///
/// They can write and join the rooms and spaces
///
/// Other peers can invite other peers to the space
///
///
#[derive(Clone)]
pub struct Space {
    id: uuid::Uuid, 
    name: String,

    // Documents
    pub info: Doc,
    pub members: Doc,
    users: Vec<String>,
    pub rooms: Arc<Mutex<Vec<Room>>>,
    pub docs: Docs,
    pub call_rooms: Arc<Mutex<Vec<CallRoom>>>,

    pub member_cache: Arc<Mutex<Vec<Member>>>,
    pending_member: Arc<Mutex<HashMap<Hash, Entry>>>,

    pub room_record_cache : Arc<Mutex<Vec<RoomRecord>>>,
    pub call_room_record_cache: Arc<Mutex<Vec<CallRoomRecord>>>,

    pending_room: Arc<Mutex<HashMap<Hash, Entry>>>,




}

impl Space {

    /// Create a completely new space
    pub async fn new(
        docs: &Docs,
        author: AuthorId,
        endpoint_id: String,
        display_name: String,
        name: impl Into<String>
        ) -> Result<Self> {
        // create a UUID for the space

        let info = docs.create().await?;
        let members = docs.create().await?;

        let id = Uuid::new_v4();

        let space = Self {
            id,
            name: name.into(),
            info,
            members,
            users: vec![],
            rooms: Arc::new(Mutex::new(Vec::new())),
            docs: docs.clone(),
            call_rooms: Arc::new(Mutex::new(Vec::new())),

            member_cache: Arc::new(Mutex::new(Vec::new())),
            pending_member: Arc::new(Mutex::new(HashMap::new())),

            room_record_cache: Arc::new(Mutex::new(Vec::new())),
            call_room_record_cache: Arc::new(Mutex::new(Vec::new())),

            pending_room: Arc::new(Mutex::new(HashMap::new())),
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
        let joined_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let member = Member {
            author_id: author.to_string(), 
            endpoint_id: endpoint_id.to_string(),
            display_name: display_name.to_string(),
            joined_at,
        };

        let key = format!("member/{author}");
        let value = postcard::to_allocvec(&member)?;

        self.members.set_bytes(author, key.into_bytes(), value).await?;
        Ok(())
    }
    
    /// Create a space from an Invite
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
            name: invite.space_name,
            info,
            members,
            users: vec![],
            rooms: Arc::new(Mutex::new(Vec::new())),
            docs: docs.clone(),
            call_rooms: Arc::new(Mutex::new(Vec::new())),

            member_cache: Arc::new(Mutex::new(Vec::new())),
            pending_member: Arc::new(Mutex::new(HashMap::new())),

            room_record_cache: Arc::new(Mutex::new(Vec::new())),
            call_room_record_cache: Arc::new(Mutex::new(Vec::new())),

            pending_room: Arc::new(Mutex::new(HashMap::new())),
        };

        space
            .insert_self_as_member(author, endpoint_id, display_name)
            .await?;

        Ok(space)
    }

    /// create an invite for the Space
    /// Should include at least one bootstrap node
    pub async fn create_invite(&self) -> Result<Invite> {
        Invite::from_space(self).await 
    }
    
    /// create the directory for the space
    /// Document for Meta aswell as the chats
    /// will be saved in that directory
    pub async fn create_directory(&self, root_path: &std::path::Path) -> Result<()> {
        let meta_dir = root_path.join(self.id.to_string()).join("meta");
        tokio::fs::create_dir_all(&meta_dir).await?;
        let invite = self.create_invite().await?;
        let code = invite.serialize_invite()?;
        tokio::fs::write(meta_dir.join("invite.txt"), code).await?;
        Ok(())
    }

    pub async fn list_members(&self, blobs: &FsStore) -> Result<Vec<Member>> {
        // get all the members
        let entries = self
            .members
            .get_many(Query::single_latest_per_key().key_prefix("member/"))
            .await?;

        let mut entries = std::pin::pin!(entries);
        let mut members = Vec::new();
        
        // go through the entries and parse as member
        while let Some(entry) = entries.next().await {
            let entry = entry?;
            let key = std::str::from_utf8(entry.key())?;
            let claim_auth = key.strip_prefix("member/").unwrap_or_default();
            if claim_auth != entry.author().to_string() {
                continue;
            }
            let bytes = blobs.blobs().get_bytes(entry.content_hash()).await?;
            let member: Member = postcard::from_bytes(&bytes)?;
            members.push(member);
        }

        Ok(members)
    }

    /// Create a completely new room
    pub async fn create_room(&self,node: &Node ,author: AuthorId, name: impl Into<String>) -> Result<Room> {
        let name = name.into();
        let room = Room::new(&self.docs, name.clone()).await?;

        let ticket = room.messages.share(ShareMode::Write, AddrInfoOptions::RelayAndAddresses).await?;

        let record = RoomRecord {
            id: room.id,
            name: name.clone(),
            ticket: ticket.to_string(),
        };

        let key = format!("room/{}", room.id);

        let value = postcard::to_allocvec(&record)?;

        self.info.set_bytes(author, key.into_bytes(), value).await?;

        self.rooms.lock().await.push(room.clone());

        let blobs = node.blobs.clone().context("blobs not created yet")?;

        let label = format!("{}/{}", self.name, room.name);
        room.watch(node, blobs, label, self.id).await?;

        let _ = node.events.send(events::AppEvent::NewRoom {
            space_id: self.id,
            room_id: room.id,
            room_name: room.name.clone(), 
        });



        Ok(room)

    }

    /// Create a new voice call room
    pub async fn create_call_room(&self, author: AuthorId, name: impl Into<String>) -> Result<CallRoom> {
        let name = name.into();
        let room = CallRoom::new(&self.docs, name.clone()).await?;
        let ticket = room.call_log.share(ShareMode::Write, AddrInfoOptions::RelayAndAddresses).await?;
        let record = CallRoomRecord {
            id: room.id,
            name: name.clone(),
            ticket: ticket.to_string(),};
        let key = format!("callroom/{}", room.id);
        let value = postcard::to_allocvec(&record)?;
        self.info.set_bytes(author, key.into_bytes(), value).await?;
        self.call_rooms.lock().await.push(room.clone());
        Ok(room)
    }

    /// initially called when joining a space just to update the in memory
    /// information on the rooms currently on that space
    pub async fn sync_rooms(&self, node: &Node, blobs: &FsStore) -> Result<Vec<JoinHandle<()>>> {
        let entries = self
            .info
            .get_many(Query::single_latest_per_key().key_prefix("room/"))
            .await?;
        
        let mut entries = std::pin::pin!(entries);

        // go through the entries and create RoomRecors
        while let Some(entry) = entries.next().await {
            let entry = entry?;
            // content may not have finished downloading yet if the entry metadata
            // synced ahead of the blob content; skip it for now, a later sync will pick it up
            let Some(bytes) = blobs.blobs().get_bytes(entry.content_hash()).await.ok() else {
                continue;
            };
            let record: RoomRecord = postcard::from_bytes(&bytes)?;

            let already_known = {
                let rooms = self.rooms.lock().await;
                rooms.iter().any(|r| r.id == record.id)
            };

            if !already_known {
                let ticket = DocTicket::from_str(&record.ticket)?;
                let room = Room::from_ticket(&self.docs, record.id, record.name, ticket).await?;
                self.rooms.lock().await.push(room);
            }
        }

        let rooms = self.rooms.lock().await;
        let mut handles = Vec::new();
        for room in rooms.iter() {
            let label = format!("{}/{}", self.name, room.name);
            handles.push(room.watch(node, blobs.clone(), label, self.id).await?);
        }
        Ok(handles)
    }

    /// initially called when joining a space just to update the in memory info on the call rooms currently on that space
    pub async fn sync_call_rooms(&self, node: &Node, blobs: &FsStore) -> Result<Vec<JoinHandle<()>>> {
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
            let record: CallRoomRecord = postcard::from_bytes(&bytes)?;
            let already_known = {
                let call_rooms = self.call_rooms.lock().await;
                call_rooms.iter().any(|r| r.id == record.id)
            };
            if !already_known {
                let ticket = DocTicket::from_str(&record.ticket)?;
                let room = CallRoom::from_ticket(&self.docs, record.id, record.name, ticket).await?;
                self.call_rooms.lock().await.push(room);
            }}
        let call_rooms = self.call_rooms.lock().await;
        let mut handles = Vec::new();
        for room in call_rooms.iter() {
            let label = format!("{}/{}", self.name, room.name);
            handles.push(room.watch(node, blobs.clone(), label, self.id).await?);
        }
        Ok(handles)
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
                if let Ok(bytes) = blobs.blobs().get_bytes(entry.content_hash()).await {
                    if let Ok(member) = postcard::from_bytes::<Member>(&bytes) {
                        self.member_cache.lock().await.push(member.clone());
                        let _ = events.send(AppEvent::NewMember { space_id, member });
                        return true;
                    }
                }
                false
            },
                SpaceEvents::RecordEvent => {
                let Ok(key) = std::str::from_utf8(entry.key()) else { return false; };

                if let Some(_room_id_str) = key.strip_prefix("room/") {
                    if let Ok(bytes) = blobs.blobs().get_bytes(entry.content_hash()).await {
                        if let Ok(record) = postcard::from_bytes::<RoomRecord>(&bytes) {
                            println!("Room discovered");
                            let already_known = {
                                let rooms = self.rooms.lock().await;
                                rooms.iter().any(|r| r.id == record.id)
                            };
                            if !already_known {
                                if let Ok(ticket) = DocTicket::from_str(&record.ticket) {
                                    if let Ok(room) = Room::from_ticket(&self.docs, record.id, record.name.clone(), ticket).await {
                                        self.rooms.lock().await.push(room.clone());
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
                    }
                    return false;
                }

                if let Some(_room_id_str) = key.strip_prefix("callroom/") {
                    if let Ok(bytes) = blobs.blobs().get_bytes(entry.content_hash()).await {
                        if let Ok(record) = postcard::from_bytes::<CallRoomRecord>(&bytes) {
                            let already_known = {
                                let call_rooms = self.call_rooms.lock().await;
                                call_rooms.iter().any(|r| r.id == record.id)
                            };
                            if !already_known {
                                if let Ok(ticket) = DocTicket::from_str(&record.ticket) {
                                    if let Ok(room) = CallRoom::from_ticket(&self.docs, record.id, record.name.clone(), ticket).await {
                                        println!("New Callroom");
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
                    }
                    return false;
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
                let stashed = pending.lock().await.remove(&hash);
                if let Some(entry) = stashed {
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
    ) -> Result<JoinHandle<()>> {
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

        Ok(handle)
    }

    pub async fn watch_info(
        &self,
        node: &Node,
        blobs: FsStore,
        label: impl Into<String>,
    ) -> Result<JoinHandle<()>> {
        let doc = self.info.clone();
        let space = self.clone();
        let pending = self.pending_room.clone();
        let label = label.into();
        let events = node.events.clone();
        let space_id = self.id().clone();

        let handle = node.watch_doc(doc, label, move |event| {
            let space = space.clone();
            let pending = pending.clone();
            let blobs = blobs.clone();
            let events = events.clone();

            async move {
                space.apply_event(SpaceEvents::RecordEvent, pending, event, &blobs, &events, space_id).await;
            }
        }).await?;

        Ok(handle)

    }
    
    /// subscribe to the info Document
    pub async fn subscribe_info(&self) -> Result<impl Stream<Item = Result<LiveEvent>>> {
        let events = self.info.subscribe().await?;
        Ok(events)
    }
    
    /// subscribe to the member document
    pub async fn subscribe_members(&self) -> Result<impl Stream<Item = Result<LiveEvent>>> {
        let events = self.members.subscribe().await?;
        Ok(events)
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

