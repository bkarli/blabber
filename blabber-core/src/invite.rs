use crate::space::Space;
use uuid::Uuid;
use serde::{Serialize, Deserialize};
use anyhow::Result;
use data_encoding::BASE32_NOPAD;
use iroh_docs::api::protocol::{AddrInfoOptions, ShareMode};


#[derive(Serialize, Deserialize)]
pub struct Invite {
    pub space_id: Uuid,
    pub space_name: String,
    pub info_ticket: String,
    pub member_ticket: String,
}

impl Invite {
    /// Create a invite string from a space
    pub async fn from_space(space: &Space) -> Result<Self> {
        let info_ticket = space.info.share(ShareMode::Read, AddrInfoOptions::RelayAndAddresses).await?;
        let member_ticket = space.members.share(ShareMode::Write, AddrInfoOptions::RelayAndAddresses).await?;

        Ok(Self {
            space_id: space.id(),
            space_name: space.name().to_string(), 
            info_ticket: info_ticket.to_string(),
            member_ticket: member_ticket.to_string(),
        })
    }
    
    /// Serialize the invite struct
    pub fn serialize_invite(&self) -> Result<String> {
        let bytes = postcard::to_allocvec(self)?;

        Ok(BASE32_NOPAD.encode(&bytes))
    }
    
    /// Deserialize the Invite from a string
    pub fn deserialize_invite(data: impl Into<String>) -> Result<Self> {
        let data = data.into();
        let bytes = BASE32_NOPAD
            .decode(data.as_bytes())
            .map_err(|e| anyhow::anyhow!("invalid base32 invite: {e}"))?;

        let invite = postcard::from_bytes(&bytes)?;
        Ok(invite)
    }
}
