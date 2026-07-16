use blabber_core::channel::ActiveVoiceCall;
use blabber_core::node::Node;
use std::sync::Mutex;
use tokio::sync::Mutex as TokioMutex;

#[path = "commands/login.rs"]
mod login;
use login::{create_identity, list_identities, login,logout};

#[path = "commands/voice_channel.rs"]
mod voice_channel;
use voice_channel::{hang_up, start_call, my_endpoint_id};

#[derive(Default)]
pub struct AppState {
    pub node: TokioMutex<Option<Node>>,
    pub active_call: Mutex<Option<ActiveVoiceCall>>,
    pub known_spaces: Mutex<Vec<crate::space::SpaceInfo>>,
}
#[path = "commands/space.rs"]
mod space;
use space::{create_server, list_servers};

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            greet,
            create_identity,
            login,
            list_identities,
            logout,
            start_call,
            hang_up,
            create_server,
            my_endpoint_id,
            list_servers
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
