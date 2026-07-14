use std::str::FromStr;

use crate::invite::Invite;
use anyhow::{Ok, Result};
use iroh_blobs::store::fs::FsStore;
use iroh_docs::{AuthorId, DocTicket, engine::LiveEvent, store::Query};
use n0_future::{Stream, StreamExt};
use crate::room::Room;
use iroh_docs::api::Doc;
use iroh_docs::protocol::Docs;
use uuid::Uuid;
use serde::{Serialize, Deserialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize)]
pub struct Member {
    pub endpoint_id: String,   // stringified EndpointId, for easy postcard/display
    pub display_name: String,
    pub joined_at: u64,
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
    rooms: Vec<Room>,
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
            rooms: vec![],
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
            rooms: vec![],
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
    pub fn create_directory() -> Result<()> {
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
