use std::{str::FromStr, sync::Arc};


use crate::{Node, invite::Invite};
use anyhow::{Ok, Result};
use iroh_blobs::store::fs::FsStore;
use iroh_docs::{AuthorId, DocTicket, api::protocol::ShareMode, engine::LiveEvent, store::Query};
use n0_future::{Stream, StreamExt};
use tokio::{sync::Mutex, task::JoinHandle};
use crate::room::Room;
use iroh_docs::api::Doc;
use iroh_docs::protocol::Docs;
use uuid::Uuid;
use serde::{Serialize, Deserialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Member {
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
pub struct Space {
    id: uuid::Uuid, 
    name: String,

    // Documents
    pub info: Doc,
    pub members: Doc,
    users: Vec<String>,
    pub rooms: Arc<Mutex<Vec<Room>>>,
}

impl Space {

    /// Create a completely new space
    pub async fn new(docs: &Docs,author: AuthorId,endpoint_id: String, display_name: String, name: impl Into<String> ) -> Result<Self> {
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
        let code = invite.serialize_invite().unwrap();
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
            let bytes = blobs.blobs().get_bytes(entry.content_hash()).await?;
            let member: Member = postcard::from_bytes(&bytes)?;
            members.push(member);
        }

        Ok(members)
    }

    /// Create a completely new room
    pub async fn create_room(&self, docs: &Docs, author: AuthorId, name: impl Into<String>) -> Result<Room> {
        let name = name.into();
        let room = Room::new(docs, name.clone()).await?;

        let ticket = room.messages.share(ShareMode::Write, Default::default()).await?;

        let record = RoomRecord {
            id: room.id,
            name: name.clone(),
            ticket: ticket.to_string(),
        };

        let key = format!("room/{}", room.id);

        let value = postcard::to_allocvec(&record)?;

        self.info.set_bytes(author, key.into_bytes(), value).await?;

        self.rooms.lock().await.push(room.clone());

        Ok(room)

    }

    /// initially called when joining a space just to update the in memory
    /// information on the rooms currently on that space
    pub async fn sync_rooms(&self, node: &Node, docs: &Docs, blobs: &FsStore) -> Result<Vec<JoinHandle<()>>> {
        let entries = self
            .info
            .get_many(Query::single_latest_per_key().key_prefix("room/"))
            .await?;
        
        let mut entries = std::pin::pin!(entries);

        // go through the entries and create RoomRecors
        while let Some(entry) = entries.next().await {
            let entry = entry?;
            let bytes = blobs.blobs().get_bytes(entry.content_hash()).await?;
            let record: RoomRecord = postcard::from_bytes(&bytes)?;

            let already_known = {
                let rooms = self.rooms.lock().await;
                rooms.iter().any(|r| r.id == record.id)
            };

            if !already_known {
                let ticket = DocTicket::from_str(&record.ticket)?;
                let room = Room::from_ticket(docs, record.id, record.name, ticket).await?;
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

