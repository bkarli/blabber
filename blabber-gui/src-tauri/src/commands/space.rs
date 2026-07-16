use tauri::{AppHandle, Manager, State};
use serde::Serialize;
use std::path::PathBuf;
use crate::AppState;

#[derive(Serialize, Clone)]
pub struct SpaceInfo {
    pub id: String,
    pub name: String,
}


pub fn spaces_dir(app: &AppHandle)-> Result<PathBuf, String>{
    app.path().app_data_dir().map_err(|error| error.to_string()).map(|dir| dir.join("spaces"))
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
 
    let space = node.create_space(name).await.map_err(|e| e.to_string())?;
 
    let root = spaces_dir(&app)?;
    space.create_directory(&root).await.map_err(|e| e.to_string())?;
 
    let info = SpaceInfo {
        id: space.id().to_string(),
        name: space.name().to_string(),
    };
 
    state.known_spaces.lock().unwrap().push(info.clone());
 
    Ok(info)
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
        .into_iter()
        .map(|space| SpaceInfo {
            id: space.id().to_string(),
            name: space.name().to_string(),
        })
        .collect();

    *state.known_spaces.lock().unwrap() = infos.clone();

    Ok(infos)
}
