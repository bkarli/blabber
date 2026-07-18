use tauri::State;
use serde::Serialize;
use blabber_core::call_rooms::CallRoom;
use uuid::Uuid;
use crate::AppState;

#[derive(Serialize, Clone)]
pub struct CallRoomInfo {
    pub id: String,
    pub name: String,
}

#[tauri::command]
pub async fn create_call_room(
    state: State<'_, AppState>,
    space_id: String,
    name: String,
    participants: Vec<String>,
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
    *room.participants.lock().await = participants;
    Ok(CallRoomInfo {
        id: room.id.to_string(),
        name: room.name.clone(),
    })
}

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

#[tauri::command]
pub async fn join_call_room(state: State<'_, AppState>, room_id: String) -> Result<(), String> {
    let room_uuid: Uuid = room_id.parse().map_err(|e| format!("invalid room id: {e}"))?;
    let node_guard = state.node.lock().await;
    let node = node_guard.as_ref().ok_or("Node not started yet")?;
    let room: CallRoom = {
        let spaces = state.spaces.lock().await;
        let mut found: Option<CallRoom> = None;
        for space in spaces.iter() {
            let call_rooms = space.call_rooms.lock().await;
            if let Some(r) = call_rooms.iter().find(|r| r.id == room_uuid) {
                found = Some(r.clone());
                break;
            }
        }
        found.ok_or("call room not found")?
    };
    let (call, _channel) = node.join_call_room(&room).await.map_err(|e| e.to_string())?;
    *state.active_call_room.lock().unwrap() = Some((room_uuid, call));
    Ok(())
}

#[tauri::command]
pub async fn leave_call_room(state: State<'_, AppState>) -> Result<(), String> {
    let taken = state.active_call_room.lock().unwrap().take();
    if let Some((room_uuid, call)) = taken {
        call.hang_up();
        let node_guard = state.node.lock().await;
        if let Some(node) = node_guard.as_ref() {
            if let Some(endpoint) = node.endpoint.as_ref() {
                let my_id = endpoint.id().to_string();
                let spaces = state.spaces.lock().await;
                for space in spaces.iter() {
                    let call_rooms = space.call_rooms.lock().await;
                    if let Some(room) = call_rooms.iter().find(|r| r.id == room_uuid) {
                        let mut participants = room.participants.lock().await;
                        participants.retain(|id| id != &my_id);
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}