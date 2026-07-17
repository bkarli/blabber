use tauri::{AppHandle, Manager, State};
use serde::Serialize;
use std::path::PathBuf;
use crate::AppState;
use blabber_core::invite::Invite;

#[derive(Serialize, Clone)]
pub struct SpaceInfo {
    pub id: String,
    pub name: String,
}

pub fn spaces_dir(
    app: &AppHandle,
) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|error| error.to_string())
        .map(|dir| dir.join("spaces"))
}

#[tauri::command]
pub async fn create_server(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<SpaceInfo, String> {
    println!("Create_Server Command called {}", name);
    let name = name.trim();
    if name.is_empty() {
        return Err("Server name cannot be empty".to_string());
    }
 
    let node_guard = state.node.lock().await;
    let node = node_guard.as_ref().ok_or("Node not started yet... please log in first")?;
 
    let space = node.create_space(name).await.map_err(|e| e.to_string())?;

    let root = spaces_dir(&app)?;

    let user_root = node
        .add_idetity_to_path(&root)
        .map_err(|error| error.to_string())?;

    space
        .create_directory(&user_root)
        .await
        .map_err(|error| error.to_string())?;

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
        .map_err(|error| error.to_string())?;

    invite
        .serialize_invite()
        .map_err(|error| error.to_string())
}


#[tauri::command]
pub async fn list_servers(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<SpaceInfo>, String> {
    let root = spaces_dir(&app)?;
    tokio::fs::create_dir_all(&root)
        .await
        .map_err(|error| error.to_string())?;
    let mut node_guard = state.node.lock().await;
    let node = node_guard
        .as_mut()
        .ok_or("Node not started yet... please log in first")?;
    let loaded_spaces = node
        .load_spaces(root)
        .await
        .map_err(|error| error.to_string())?;

    let infos: Vec<SpaceInfo> = loaded_spaces
        .iter()
        .map(|space| SpaceInfo {
            id: space.id().to_string(),
            name: space.name().to_string(),
        })
        .collect();
    *state.spaces.lock().await = loaded_spaces;
    Ok(infos)
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
    let invite = Invite::deserialize_invite(ticket.to_string()).map_err(|error| error.to_string())?;

    let node_guard = state.node.lock().await;
    let node = node_guard.as_ref().ok_or("Node not started yet... please log in first")?;

    let space = node.join_space(invite).await.map_err(|error| error.to_string())?;

    let root = spaces_dir(&app)?;
    let user_root = node.add_idetity_to_path(&root).map_err(|error| error.to_string())?;
    space.create_directory(&user_root).await.map_err(|error| error.to_string())?;

    // discover any rooms already published in the space's info doc
    let blobs = node.blobs.clone().ok_or("Blobs not created yet")?;
    space
        .sync_rooms(node, &blobs)
        .await
        .map_err(|error| error.to_string())?;

    let info = SpaceInfo {
        id: space.id().to_string(),
        name: space.name().to_string(),
    };
    state.spaces.lock().await.push(space);
    Ok(info)
}
