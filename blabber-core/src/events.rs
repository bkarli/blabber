// Events for the Frontend
use uuid::Uuid;
use crate::{room::Message, space::Member};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum AppEvent {
    NewMessage { // whenever a new message is seen
        space_id: Uuid,
        room_id: Uuid,
        message: Message
    },
    NewMember { // emitted for new member/relay and an existing ones record being updated
        space_id: Uuid,
        member: Member,
    },
    MemberLeft { // emitted on tombstone entry
        space_id: Uuid,
        author_id: String,
    },
    NewRoom { // emitted on room creation (doing / receiving from others)
        space_id: Uuid,
        room_id: Uuid,
        room_name: String,
    },
    NewCallRoom { // same here
        space_id: Uuid,
        room_id: Uuid,
        room_name: String,
    },
    NewCallParticipant { // sent on joining (self or others)
        space_id: Uuid,
        room_id: Uuid,
        endpoint_id: String,
    },
    CallParticipantLeft { // sent on leaving (self or others) when connection actually is dropped.
        space_id: Uuid,
        room_id: Uuid,
        endpoint_id: String,
    },
    MemberOnline { // gossip swarm neighbor for the members doc came up
        space_id: Uuid,
        endpoint_id: String,
    },
    MemberOffline { // gossip swarm neighbor for the members doc went down
        space_id: Uuid,
        endpoint_id: String,
    },
}
