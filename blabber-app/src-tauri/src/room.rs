use serde::Serialize;
use tauri::State;
use uuid::Uuid;
use base64::{engine::general_purpose::STANDARD as base64_engine, Engine as _};

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
        .create_room(&node, author, name)
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
#[serde(tag= "kind")]
pub enum MessageContentInfo {
    Text { text: String },
    Image {filename: String, mime: String, data_base64: String },
}


#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MessageInfo {
    pub author: String,
    pub content: MessageContentInfo,
    pub sent_at: u64,
}

fn to_message_info(message: blabber_core::room::Message) -> MessageInfo {
    let content = match message.content {
        blabber_core::room::MessageContent::Text(text) => MessageContentInfo::Text { text },
        blabber_core::room::MessageContent::Image { filename, mime, data } => {
            MessageContentInfo::Image { filename, mime, data_base64: base64_engine.encode(data) }
        }
    };
    MessageInfo { author: message.author, content, sent_at: message.sent_at }}

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
pub async fn send_image(
    state: State<'_, AppState>,
    space_id: String,
    room_id: String,
    path: String,
) -> Result<(), String> {
    let node_guard = state.node.lock().await;
    let node = node_guard.as_ref().ok_or("Node not started yet")?;
    let author = node.author.ok_or("Author not created yet")?;

    let spaces = state.spaces.lock().await;
    let space = spaces.iter().find(|s| s.id().to_string() == space_id).ok_or("Space not found")?;

    let room_uuid: Uuid = room_id.parse().map_err(|e| format!("invalid room id: {e}"))?;
    let rooms = space.rooms.lock().await;
    let room = rooms.iter().find(|r| r.id == room_uuid).ok_or("Room not found")?;

    let data = tokio::fs::read(&path).await.map_err(|e| e.to_string())?;
    let filename = std::path::Path::new(&path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("image")
        .to_string();
    let mime = mime_guess::from_path(&path).first_or_octet_stream().to_string();

    room.send_image(author, filename, mime, data).await.map_err(|e| e.to_string())?;
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
    Ok(messages.into_iter().map(to_message_info).collect())
}

#[tauri::command]
pub async fn get_my_author_id(state: State<'_, AppState>) -> Result<String, String> {
    let guard = state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not started yet")?;
    let author = node.author.ok_or("Author not created yet")?;

    Ok(author.to_string())
}

#[tauri::command]
pub async fn my_endpoint_id(state: State<'_, AppState>) -> Result<String, String> {
    let guard = state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not started yet")?;
    let endpoint = node.endpoint.as_ref().ok_or("Endpoint not created yet")?;

    Ok(endpoint.id().to_string())
}
