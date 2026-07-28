use blabber_core::identity::Identity;
use blabber_core::node::Node;
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};
use zeroize::Zeroizing;
use crate::space::spaces_dir;

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
    let safe_name = blabber_core::identity::sanitize_path_component(display_name)
        .ok_or("Invalid display name")?;
    Ok(directory.join(format!("{safe_name}.bin")))
}

#[tauri::command]
pub async fn delete_identity(
    app: AppHandle,
    state: State<'_, AppState>,
    display_name: String,
) -> Result<(), String> {
    // shut down the running node first, if any
    let node = state.node.lock().await.take();
    state.spaces.lock().await.clear();
    if let Some(node) = node {
        node.shutdown().await.map_err(|error| error.to_string())?;
    }

    let path = identity_path(&app, display_name.trim())?;
    if path.exists() {
        std::fs::remove_file(&path).map_err(|error| error.to_string())?;
    }

    Ok(())
}

async fn start_node_for_identity(
    app: &AppHandle,
    state: &State<'_, AppState>,
    identity: Identity,
    password: &str,
    identity_path: &std::path::Path,
) -> Result<(), String> {
    let blobs_path = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("blobs");
    fs::create_dir_all(&blobs_path).map_err(|error| error.to_string())?;

    let mut node = Node::new(identity);
    node.run(blobs_path).await.map_err(|e| e.to_string())?;

    // apply any previously saved audio device preference. a device that no
    // longer exists shouldnt block login, fall back to the OS default
    if let Ok(audio_settings) = crate::audio::load_audio_settings(app) {
        if let Err(e) = node.sound.set_input_device(audio_settings.input_device) {
            eprintln!("failed to apply saved input device: {e:#}");
        }
        if let Err(e) = node.sound.set_output_device(audio_settings.output_device) {
            eprintln!("failed to apply saved output device: {e:#}");
        }
    }

    if let Some(updated_identity) = node.identity_with_author() {
        updated_identity
            .store(password, identity_path)
            .map_err(|error| error.to_string())?;
    }

    crate::event_bridge::spawn_event_bridge(app.clone(), &node);
    let spaces_root = spaces_dir(app)?;
    fs::create_dir_all(&spaces_root).map_err(|error| error.to_string())?;

    let loaded_spaces = node
        .load_spaces(spaces_root)
        .await
        .map_err(|e| e.to_string())?;
    *state.spaces.lock().await = loaded_spaces;
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
    let password = Zeroizing::new(password);
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
        .store(&password, &path)
        .map_err(|error| error.to_string())?;

    let display_name_for_return = identity.displayName.clone();

    start_node_for_identity(&app, &state, identity, &password, &path).await?;

    Ok(display_name_for_return)
}

#[tauri::command]
pub async fn login(
    app: AppHandle,
    state: State<'_, AppState>,
    display_name: String,
    password: String,
) -> Result<String, String> {
    let password = Zeroizing::new(password);
    let path = identity_path(&app, display_name.trim())?;
    if !path.exists() {
        return Err("Identity does not exist".to_string());
    }
    let identity = Identity::load_from_disk(&path, &password)
        .map_err(|error| error.to_string())?;

    let display_name_for_return = identity.displayName.clone();

    start_node_for_identity(&app, &state, identity, &password, &path).await?;

    Ok(display_name_for_return)
}

#[tauri::command]
pub async fn logout(
    state: State<'_, AppState>,
) -> Result<(), String> {
    let node = state.node.lock().await.take();

    state.spaces.lock().await.clear();

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
