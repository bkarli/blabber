use tauri::State;
use serde::Serialize;
use blabber_core::call_rooms::CallRoom;
use blabber_core::node::Node;
use blabber_core::space::Space;
use uuid::Uuid;
use crate::AppState;
use crate::result_ext::ResultExt;

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
    let spaces = state.spaces.lock().await;
    let space = spaces
        .iter()
        .find(|s| s.id() == space_uuid)
        .ok_or("space not found")?;
    let author = space
        .author()
        .ok_or("this space has no writable author (blind relay?)")?;
    let room = space
        .create_call_room(node, author, name)
        .await
        .stringify_err()?;
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
    room.list_active_members(blobs).await.stringify_err()
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

    // one call per client: properly leave the current one first. If the join
    // below fails, the client ends up in no call at all.
    hang_up_active_call(&state, node).await;

    let (call, _channel) = node.join_call_room(space_id, &room).await.stringify_err()?;
    *state.active_call_room.lock().unwrap() = Some((room_uuid, call));
    Ok(())
}

/// Leaves the client's currently active call room, if any, and tears down its mesh call.
#[tauri::command]
pub async fn leave_call_room(state: State<'_, AppState>) -> Result<(), String> {
    let node_guard = state.node.lock().await;
    match node_guard.as_ref() {
        Some(node) => hang_up_active_call(&state, node).await,
        // no node to publish through: just stop the local audio pipeline
        None => drop(state.active_call_room.lock().unwrap().take()),
    }
    Ok(())
}

/// Ends this client's active call, if any. Every path that ends a call goes
/// through here (leaving, switching rooms, leaving the space, logout), so a
/// call never outlives its room, space or node.
pub(crate) async fn hang_up_active_call(state: &AppState, node: &Node) {
    let Some((room_uuid, call)) = state.active_call_room.lock().unwrap().take() else {
        return;
    };
    if let Err(e) = node.leave_call_room(room_uuid, call).await {
        eprintln!("failed to publish call departure for room {room_uuid}: {e:#}");
    }
}

/// Whether this client's active call is in a room of `space_id`.
pub(crate) fn active_call_in_space(state: &AppState, node: &Node, space_id: Uuid) -> bool {
    let Some(room_uuid) = state.active_call_room.lock().unwrap().as_ref().map(|(id, _)| *id) else {
        return false;
    };
    node.room_spaces.lock().unwrap().get(&room_uuid) == Some(&space_id)
}

/// Mutes or unmutes this client's mic. an engine wide setting, since only
/// one call can be active at a time.
#[tauri::command]
pub async fn set_muted(state: State<'_, AppState>, muted: bool) -> Result<(), String> {
    let node_guard = state.node.lock().await;
    let node = node_guard.as_ref().ok_or("Node not started yet")?;
    node.sound.set_muted(muted);
    Ok(())
}
