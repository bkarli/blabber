use std::fs;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use blabber_core::sound::AudioDeviceInfo;
use crate::AppState;
use crate::result_ext::ResultExt;

/// Persistent device selection, global to the app, stored alongside identities/spaces.
#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct AudioSettings {
    pub(crate) input_device: Option<String>,
    pub(crate) output_device: Option<String>,
}

fn audio_settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = app.path().app_data_dir().stringify_err()?;
    fs::create_dir_all(&app_data_dir).stringify_err()?;
    Ok(app_data_dir.join("audio_settings.json"))
}

/// Loads the persisted device preferences. Missing or unreadable
/// settings are treated as no preference rather than an error.
pub(crate) fn load_audio_settings(app: &AppHandle) -> Result<AudioSettings, String> {
    let path = audio_settings_path(app)?;
    if !path.is_file() {
        return Ok(AudioSettings::default());
    }
    let raw = fs::read_to_string(&path).stringify_err()?;
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

fn save_audio_settings(app: &AppHandle, settings: &AudioSettings) -> Result<(), String> {
    let path = audio_settings_path(app)?;
    let raw = serde_json::to_string_pretty(settings).stringify_err()?;
    fs::write(path, raw).stringify_err()
}

#[derive(Serialize)]
pub struct AudioDevicesResponse {
    pub inputs: Vec<AudioDeviceInfo>,
    pub outputs: Vec<AudioDeviceInfo>,
    pub selected_input: Option<String>,
    pub selected_output: Option<String>,
}

/// Lists the OS level input/output devices cpal can see, and which
/// the user has selected.
#[tauri::command]
pub async fn list_audio_devices(state: State<'_, AppState>) -> Result<AudioDevicesResponse, String> {
    let node_guard = state.node.lock().await;
    let node = node_guard.as_ref().ok_or("Node not started yet... please log in first")?;

    let inputs = node.sound.list_input_devices().stringify_err()?;
    let outputs = node.sound.list_output_devices().stringify_err()?;

    Ok(AudioDevicesResponse {
        inputs,
        outputs,
        selected_input: node.sound.input_device(),
        selected_output: node.sound.output_device(),
    })
}

/// Sets or clears the input device and persists the choice.
#[tauri::command]
pub async fn set_input_device(
    app: AppHandle,
    state: State<'_, AppState>,
    device_name: Option<String>,
) -> Result<(), String> {
    let node_guard = state.node.lock().await;
    let node = node_guard.as_ref().ok_or("Node not started yet... please log in first")?;
    node.sound.set_input_device(device_name.clone()).stringify_err()?;

    let mut settings = load_audio_settings(&app)?;
    settings.input_device = device_name;
    save_audio_settings(&app, &settings)
}

/// Sets or clears the output device and persists the choice.
#[tauri::command]
pub async fn set_output_device(
    app: AppHandle,
    state: State<'_, AppState>,
    device_name: Option<String>,
) -> Result<(), String> {
    let node_guard = state.node.lock().await;
    let node = node_guard.as_ref().ok_or("Node not started yet... please log in first")?;
    node.sound.set_output_device(device_name.clone()).stringify_err()?;

    let mut settings = load_audio_settings(&app)?;
    settings.output_device = device_name;
    save_audio_settings(&app, &settings)
}

/// Plays a bundled sound effect by name. See assets/*.mp3
#[tauri::command]
pub async fn play_sound_effect(app: AppHandle, state: State<'_, AppState>, name: String) -> Result<(), String> {
    let node_guard = state.node.lock().await;
    let node = node_guard.as_ref().ok_or("Node not started yet... please log in first")?;

    let resource_dir = app.path().resource_dir().stringify_err()?;
    let sound_path = resource_dir.join("assets/sounds").join(format!("{name}.mp3"));

    node.sound
        .play_sound_effect(&name, || fs::read(&sound_path).map_err(Into::into))
        .stringify_err()
}
