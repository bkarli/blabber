use blabber_core::identity::Identity;
use blabber_core::node::Node;
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};

use crate::AppState;

fn identities_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;

    let identities_dir = app_data_dir.join("identities");
    fs::create_dir_all(&identities_dir)
        .map_err(|error| error.to_string())?;
    Ok(identities_dir)
}

fn identity_path(
    app: &AppHandle,
    display_name: &str,
) -> Result<PathBuf, String> {
    let directory = identities_dir(app)?;
    let safe_name: String = display_name
        .chars()
        .filter(|character| {
            character.is_alphanumeric()
                || *character == '-'
                || *character == '_'
        })
        .collect();
    if safe_name.is_empty() {
        return Err("Invalid display name".to_string());
    }
    Ok(directory.join(format!("{safe_name}.bin")))
}


async fn start_node_for_identity(
    app: &AppHandle,
    state: &State<'_, AppState>,
    identity: Identity,
) -> Result<(), String> {
    let blobs_path = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("blobs");
    fs::create_dir_all(&blobs_path).map_err(|error| error.to_string())?;

    let mut node = Node::new(identity);
    let app_for_event = app.clone();
    node.set_incoming_call_handler(move |peer_id: String| {
        use tauri::Emitter;
        let _ = app_for_event.emit("incoming_call", peer_id);
    });
    node.run(blobs_path).await.map_err(|e| e.to_string())?;
    *state.node.lock().await = Some(node);
    Ok(())
}

#[tauri::command]
pub async fn create_identity(
    app: AppHandle,
    state: State<'_, AppState>,
    display_name: String,
    password: String,
) -> Result<String, String> {
    let display_name = display_name.trim();
    if display_name.is_empty() {
        return Err("Display name cannot be empty".to_string());
    }
    if password.is_empty() {
        return Err("Password cannot be empty".to_string());
    }
    let path = identity_path(&app, display_name)?;
    if path.exists() {
        return Err("An identity with this name already exists".to_string());
    }

    let identity = Identity::new(display_name);
    identity
        .store(password, path)
        .map_err(|error| error.to_string())?;

    let display_name_for_return = identity.displayName.clone();

    start_node_for_identity(&app, &state, identity).await?;

    Ok(display_name_for_return)
}

#[tauri::command]
pub async fn login(
    app: AppHandle,
    state: State<'_, AppState>,
    display_name: String,
    password: String,
) -> Result<String, String> {
    let path = identity_path(&app, display_name.trim())?;
    if !path.exists() {
        return Err("Identity does not exist".to_string());
    }
    let identity = Identity::load_from_disk(path, password)
        .map_err(|error| error.to_string())?;

    let display_name_for_return = identity.displayName.clone();

    start_node_for_identity(&app, &state, identity).await?;

    Ok(display_name_for_return)
}

#[tauri::command]
pub async fn logout(
    state: State<'_, AppState>,
) -> Result<(), String> {
    let node = state.node.lock().await.take();

    if let Some(node) = node {
        node.shutdown()
            .await
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub fn list_identities(
    app: AppHandle,
) -> Result<Vec<String>, String> {
    let directory = identities_dir(&app)?;
    let mut identities = Vec::new();
    for entry in fs::read_dir(directory)
        .map_err(|error| error.to_string())?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str())
            == Some("bin")
        {
            if let Some(name) =
                path.file_stem().and_then(|name| name.to_str())
            {
                identities.push(name.to_string());
            }
        }
    }

    identities.sort();

    Ok(identities)
}