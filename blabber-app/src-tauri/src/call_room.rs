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
    name: String,
    participants: Vec<String>,
) -> Result<CallRoomInfo, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Room name cannot be empty".to_string());
    }
    let node_guard = state.node.lock().await;
    let node = node_guard.as_ref().ok_or("Node not started yet")?;
    let docs = node.docs.as_ref().ok_or("Docs engine not ready")?;
    let room = CallRoom::new(docs, name).await.map_err(|e| e.to_string())?;
    *room.participants.lock().await = participants;

    let info = CallRoomInfo {
        id: room.id.to_string(),
        name: room.name.clone(),
    };
    state.call_rooms.lock().await.push(room);
    Ok(info)
}

#[tauri::command]
pub async fn list_call_rooms(state: State<'_, AppState>) -> Result<Vec<CallRoomInfo>, String> {
    let rooms = state.call_rooms.lock().await;
    Ok(rooms
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
    let rooms = state.call_rooms.lock().await;
    let room = rooms
        .iter()
        .find(|r| r.id == room_uuid)
        .ok_or("call room not found")?;
    let (call, _channel) = node.join_call_room(room).await.map_err(|e| e.to_string())?;
    *state.active_call_room.lock().unwrap() = Some((room_uuid, call));
    Ok(())
}

#[tauri::command]
pub fn leave_call_room(state: State<'_, AppState>) -> Result<(), String> {
    if let Some((_, call)) = state.active_call_room.lock().unwrap().take() {
        call.hang_up();
    }
    Ok(())
}