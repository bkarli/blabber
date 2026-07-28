use tauri::State;
use serde::Serialize;
use blabber_core::call_rooms::CallRoom;
use blabber_core::space::Space;
use uuid::Uuid;
use crate::AppState;

#[derive(Serialize, Clone)]
pub struct CallRoomInfo {
    pub id: String,
    pub name: String,
}

/// Finds a call room by id across every space, returning the owning space's id alongside it.
async fn find_call_room(spaces: &[Space], room_id: Uuid) -> Option<(Uuid, CallRoom)> {
    for space in spaces {
        let call_rooms = space.call_rooms.lock().await;
        if let Some(room) = call_rooms.iter().find(|r| r.id == room_id) {
            return Some((space.id(), room.clone()));
        }
    }
    None
}

/// Creates a new call room in a space and publishes it for other members to discover.
#[tauri::command]
pub async fn create_call_room(
    state: State<'_, AppState>,
    space_id: String,
    name: String,
) -> Result<CallRoomInfo, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Room name cannot be empty".to_string());
    }
    let space_uuid: Uuid = space_id.parse().map_err(|e| format!("invalid space id: {e}"))?;
    let node_guard = state.node.lock().await;
    let node = node_guard.as_ref().ok_or("Node not started yet")?;
    let author = node.author.ok_or("author not created yet")?;
    let spaces = state.spaces.lock().await;
    let space = spaces
        .iter()
        .find(|s| s.id() == space_uuid)
        .ok_or("space not found")?;
    let room = space
        .create_call_room(author, name)
        .await
        .map_err(|e| e.to_string())?;
    Ok(CallRoomInfo {
        id: room.id.to_string(),
        name: room.name.clone(),
    })
}

/// Lists the call rooms known to a space.
#[tauri::command]
pub async fn list_call_rooms(
    state: State<'_, AppState>,
    space_id: String,
) -> Result<Vec<CallRoomInfo>, String> {
    let space_uuid: Uuid = space_id.parse().map_err(|e| format!("invalid space id: {e}"))?;
    let spaces = state.spaces.lock().await;
    let space = spaces
        .iter()
        .find(|s| s.id() == space_uuid)
        .ok_or("space not found")?;
    let call_rooms = space.call_rooms.lock().await;
    Ok(call_rooms
        .iter()
        .map(|r| CallRoomInfo {
            id: r.id.to_string(),
            name: r.name.clone(),
        })
        .collect())
}

/// Lists the endpoint ids currently marked active in a call room's roster.
#[tauri::command]
pub async fn list_call_participants(
    state: State<'_, AppState>,
    room_id: String,
) -> Result<Vec<String>, String> {
    let room_uuid: Uuid = room_id.parse().map_err(|e| format!("invalid room id: {e}"))?;

    let node_guard = state.node.lock().await;
    let node = node_guard.as_ref().ok_or("Node not started yet")?;
    let blobs = node.blobs.clone().ok_or("blobs not created yet")?;

    let spaces = state.spaces.lock().await;
    let (_, room) = find_call_room(&spaces, room_uuid).await.ok_or("call room not found")?;
    room.list_active_members(blobs).await.map_err(|e| e.to_string())
}

/// Joins a call room's mesh call and stores the resulting live call as this client's active call.
#[tauri::command]
pub async fn join_call_room(state: State<'_, AppState>, room_id: String) -> Result<(), String> {
    let room_uuid: Uuid = room_id.parse().map_err(|e| format!("invalid room id: {e}"))?;
    let node_guard = state.node.lock().await;
    let node = node_guard.as_ref().ok_or("Node not started yet")?;

    let (space_id, room): (Uuid, CallRoom) = {
        let spaces = state.spaces.lock().await;
        find_call_room(&spaces, room_uuid).await.ok_or("call room not found")?
    };

    let (call, _channel) = node.join_call_room(space_id, &room).await.map_err(|e| e.to_string())?;
    *state.active_call_room.lock().unwrap() = Some((room_uuid, call));
    Ok(())
}

/// Leaves the client's currently active call room, if any, and tears down its mesh call.
#[tauri::command]
pub async fn leave_call_room(state: State<'_, AppState>) -> Result<(), String> {
    // stop our local audio pipeline and drop the mesh connections for this room
    let Some((room_uuid, call)) = state.active_call_room.lock().unwrap().take() else {
        return Ok(());
    };
    call.hang_up();

    let node_guard = state.node.lock().await;
    let Some(node) = node_guard.as_ref() else {
        return Ok(());
    };
    node.active_call_rooms.lock().unwrap().remove(&room_uuid);
    node.room_spaces.lock().unwrap().remove(&room_uuid);

    // publish our departure so other members' rosters update
    let Some(endpoint) = node.endpoint.as_ref() else {
        return Ok(());
    };
    let my_id = endpoint.id().to_string();
    let spaces = state.spaces.lock().await;
    let Some((space_id, room)) = find_call_room(&spaces, room_uuid).await else {
        return Ok(());
    };
    if let Some(author) = node.author {
        let _ = room.set_membership(author, my_id.clone(), false).await;
    }
    let _ = node.events.send(blabber_core::events::AppEvent::CallParticipantLeft {
        space_id,
        room_id: room_uuid,
        endpoint_id: my_id,
    });
    Ok(())
}

/// Mutes or unmutes this client's mic in its currently active call, if any.
#[tauri::command]
pub async fn set_muted(state: State<'_, AppState>, muted: bool) -> Result<(), String> {
    let guard = state.active_call_room.lock().unwrap();
    if let Some((_, call)) = guard.as_ref() {
        call.set_muted(muted);
    }
    Ok(())
}

