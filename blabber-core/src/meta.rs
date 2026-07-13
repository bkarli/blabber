// meta data of a space

use std::collections::HashMap;

use iroh_docs::DocTicket;
use crate::invite::Invite;
use uuid::Uuid;

/// Store meta data of the space
/// In form of a shared document
/// Everyone can edit that meta data
/// document
pub struct Meta {
    info: DocTicket,
    members: DocTicket,
    rooms: HashMap<String, DocTicket>,
}

impl Meta {
    pub fn from_invite(invite: &Invite) {
        
    }
}
