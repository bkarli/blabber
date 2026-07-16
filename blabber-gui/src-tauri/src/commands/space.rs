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
pub fn list_servers(state: State<'_, AppState>) -> Result<Vec<SpaceInfo>, String> {
    Ok(state.known_spaces.lock().unwrap().clone())
}
