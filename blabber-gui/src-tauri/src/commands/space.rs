use tauri::State;
use serde::Serialize;
use crate::AppState;

#[derive(Serialize)]
pub struct SpaceInfo {
    pub id: String,
    pub name: String,
}

#[tauri::command]
pub async fn create_server(state: State<'_, AppState>, name: String)-> Result<SpaceInfo, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(String::from("Server name is empty"));
    }
    let node_guard = state.node.lock().await;
    let node = node_guard.as_ref().ok_or("No node started.. login first")?;
    let space = node.create_space(name).await.map_err(|e| e.to_string())?;
    Ok(SpaceInfo {
        id: space.id().to_string(),
        name: space.name().to_string(),
    })
}