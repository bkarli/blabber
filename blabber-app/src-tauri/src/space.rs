use tauri::{AppHandle, Manager, State};
use serde::Serialize;
use std::path::PathBuf;
use crate::AppState;
use crate::result_ext::ResultExt;
use blabber_core::invite::Invite;
use blabber_core::space::Member;
use uuid::Uuid;

#[derive(Serialize, Clone)]
pub struct SpaceInfo {
    pub id: String,
    pub name: String,
}

pub fn spaces_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .stringify_err()
        .map(|dir| dir.join("spaces"))
}

#[tauri::command]
pub async fn create_server(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<SpaceInfo, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Server name cannot be empty".to_string());
    }

    let node_guard = state.node.lock().await;
    let node = node_guard.as_ref().ok_or("Node not started yet... please log in first")?;

    let space = node.create_space(name).await.stringify_err()?;

    let root = spaces_dir(&app)?;

    let user_root = node
        .identity_scoped_path(&root)
        .stringify_err()?;

    space
        .create_directory(&user_root, &node.local_storage_key())
        .await
        .stringify_err()?;

    let info = SpaceInfo {
        id: space.id().to_string(),
        name: space.name().to_string(),
    };
    state.spaces.lock().await.push(space);

    Ok(info)
}

#[tauri::command]
pub async fn get_invite(
    state: State<'_, AppState>,
    space_id: String,
) -> Result<String, String> {
    let spaces = state.spaces.lock().await;

    let space = spaces
        .iter()
        .find(|space| space.id().to_string() == space_id)
        .ok_or("Space not found")?;

    let invite = space
        .create_invite()
        .await
        .stringify_err()?;

    invite
        .serialize_invite()
        .stringify_err()
}

#[tauri::command]
pub async fn get_relay_invite(
    state: State<'_, AppState>,
    space_id: String,
) -> Result<String, String> {
    let spaces = state.spaces.lock().await;

    let space = spaces
        .iter()
        .find(|space| space.id().to_string() == space_id)
        .ok_or("Space not found")?;

    let invite = space
        .create_relay_invite()
        .await
        .stringify_err()?;

    invite
        .serialize_invite()
        .stringify_err()
}

#[tauri::command]
pub async fn list_servers(state: State<'_, AppState>) -> Result<Vec<SpaceInfo>, String> {
    let spaces = state.spaces.lock().await;
    Ok(spaces
        .iter()
        .map(|space| SpaceInfo {
            id: space.id().to_string(),
            name: space.name().to_string(),
        })
        .collect())
}

#[tauri::command]
pub async fn join_space(
    app: AppHandle,
    state: State<'_, AppState>,
    ticket: String,
) -> Result<SpaceInfo, String> {
    let ticket = ticket.trim();
    if ticket.is_empty() {
        return Err("Invite ticket cannot be empty".to_string());
    }
    let invite = Invite::deserialize_invite(ticket.to_string()).stringify_err()?;

    let node_guard = state.node.lock().await;
    let node = node_guard.as_ref().ok_or("Node not started yet... please log in first")?;

    let space = node.join_space(invite).await.stringify_err()?;

    let root = spaces_dir(&app)?;
    let user_root = node.identity_scoped_path(&root).stringify_err()?;
    space.create_directory(&user_root, &node.local_storage_key()).await.stringify_err()?;

    // discover any rooms already published in the space's info doc
    let blobs = node.blobs.clone().ok_or("Blobs not created yet")?;
    space
        .sync_rooms(node, &blobs)
        .await
        .stringify_err()?;

    space
        .sync_call_rooms(&blobs)
        .await
        .stringify_err()?;

    let info = SpaceInfo {
        id: space.id().to_string(),
        name: space.name().to_string(),
    };
    state.spaces.lock().await.push(space);
    Ok(info)
}

#[tauri::command]
pub async fn leave_space(
    app: AppHandle,
    state: State<'_, AppState>,
    space_id: String,
) -> Result<(), String> {
    let space_uuid = space_id.parse::<Uuid>().stringify_err()?;

    let node_guard = state.node.lock().await;
    let node = node_guard.as_ref().ok_or("Node not started yet... please log in first")?;

    let root = spaces_dir(&app)?;
    node.leave_space(space_uuid, root).await.stringify_err()?;

    state.spaces.lock().await.retain(|space| space.id() != space_uuid);
    Ok(())
}

#[tauri::command]
pub async fn list_members(state: State<'_, AppState>, space_id: String) -> Result<Vec<Member>, String> {
    let space_uuid = space_id.parse::<Uuid>().stringify_err()?;
    let node_guard = state.node.lock().await;
    let node = node_guard.as_ref().ok_or("Node not started yet")?;
    let blobs = node.blobs.as_ref().ok_or("Blobs not ready")?;
    let spaces = state.spaces.lock().await;
    let space = spaces.iter().find(|space| space.id() == space_uuid).ok_or("space not found")?;
    space.list_members(blobs).await.stringify_err()
}
