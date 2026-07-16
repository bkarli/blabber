use serde::Serialize;
use tauri::State;

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