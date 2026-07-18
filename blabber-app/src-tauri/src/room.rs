use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use crate::AppState;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RoomInfo {
    pub id: String,
    pub name: String,
}

#[tauri::command]
pub async fn list_rooms(
    state: State<'_, AppState>,
    space_id: String,
) -> Result<Vec<RoomInfo>, String> {
    let spaces = state.spaces.lock().await;

    let space = spaces
        .iter()
        .find(|space| space.id().to_string() == space_id)
        .ok_or("Space not found")?;

    let rooms = space.rooms.lock().await;

    let result = rooms
        .iter()
        .map(|room| RoomInfo {
            id: room.id.to_string(),
            name: room.name.clone(),
        })
        .collect();

    Ok(result)
}

#[tauri::command]
pub async fn create_room(
    state: State<'_, AppState>,
    space_id: String,
    name: String,
) -> Result<RoomInfo, String> {
    let name = name.trim();

    if name.is_empty() {
        return Err("Room name cannot be empty".to_string());
    }

    let node_guard = state.node.lock().await;

    let node = node_guard
        .as_ref()
        .ok_or("Node not started yet")?;

    let author = node
        .author
        .ok_or("Author not created yet")?;

    let spaces = state.spaces.lock().await;

    let space = spaces
        .iter()
        .find(|space| space.id().to_string() == space_id)
        .ok_or("Space not found")?;

    let room = space
        .create_room(author, name)
        .await
        .map_err(|error| error.to_string())?;
    println!(
        "Room created: {} ({}) in space {}",
        room.name,
        room.id,
        space_id
    );

    Ok(RoomInfo {
        id: room.id.to_string(),
        name: room.name.clone(),
    })
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MessageInfo {
    pub author: String,
    pub content: String,
    pub sent_at: u64,
}

#[tauri::command]
pub async fn send_message(
    state: State<'_, AppState>,
    space_id: String,
    room_id: String,
    content: String,
) -> Result<(), String> {
    let content = content.trim();
    if content.is_empty() {
        return Err("Message cannot be empty".to_string());
    }

    let node_guard = state.node.lock().await;
    let node = node_guard.as_ref().ok_or("Node not started yet")?;
    let author = node.author.ok_or("Author not created yet")?;

    let spaces = state.spaces.lock().await;
    let space = spaces
        .iter()
        .find(|s| s.id().to_string() == space_id)
        .ok_or("Space not found")?;

    let room_uuid = Uuid::parse_str(&room_id).map_err(|e| e.to_string())?;
    let rooms = space.rooms.lock().await;
    let room = rooms.iter().find(|r| r.id == room_uuid).ok_or("Room not found")?;

    room.send_message(author, content).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn list_messages(
    state: State<'_, AppState>,
    space_id: String,
    room_id: String,
) -> Result<Vec<MessageInfo>, String> {
    let node_guard = state.node.lock().await;
    let node = node_guard.as_ref().ok_or("Node not started yet")?;
    let blobs = node.blobs.clone().ok_or("Blobs not created yet")?;

    let spaces = state.spaces.lock().await;
    let space = spaces
        .iter()
        .find(|s| s.id().to_string() == space_id)
        .ok_or("Space not found")?;

    let room_uuid = Uuid::parse_str(&room_id).map_err(|e| e.to_string())?;
    let rooms = space.rooms.lock().await;
    let room = rooms.iter().find(|r| r.id == room_uuid).ok_or("Room not found")?;

    let messages = room.list_messages(blobs).await.map_err(|e| e.to_string())?;

    Ok(messages
        .into_iter()
        .map(|m| MessageInfo { author: m.author, content: m.content, sent_at: m.sent_at })
        .collect())
}
