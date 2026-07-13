use crate::invite::Invite;
use uuid::Uuid;

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

}

impl Space {
    pub fn new(name: impl Into<String>) -> Self {
        let id = Uuid::new_v4();
        Self {
            id: id,
            name: name.into(),
        }
    }

    pub fn from_invite() -> Self {
        todo!()
    }
    
    // create a invite 
    pub fn create_invite(&self) -> Invite {
        todo!()
    }
}
