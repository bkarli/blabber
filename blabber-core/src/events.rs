// Events for the Frontend
use uuid::Uuid;
use crate::{room::Message, space::Member};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum AppEvent {
    NewMessage {
        space_id: Uuid,
        room_id: Uuid,
        message: Message
    },
    NewMember {
        space_id: Uuid,
        member: Member,
    },
    MemberLeft {
        space_id: Uuid,
        author_id: String,
    },
    NewRoom {
        space_id: Uuid,
        room_id: Uuid,
        room_name: String,
    },
    NewCallRoom {
        space_id: Uuid,
        room_id: Uuid,
        room_name: String,
    },
    NewCallParticipant {
        space_id: Uuid,
        room_id: Uuid,
        endpoint_id: String,
    },
    CallParticipantLeft {
        space_id: Uuid,
        room_id: Uuid,
        endpoint_id: String,
    },
}
