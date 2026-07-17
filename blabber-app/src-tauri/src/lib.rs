use blabber_core::node::Node;
use blabber_core::space::Space;
use std::sync::Mutex;
use tokio::sync::Mutex as TokioMutex;
use tokio::sync::oneshot;

mod login;
mod space;
mod room;
mod voice_channel;

use login::{create_identity, list_identities, login, logout, delete_identity};
use space::{create_server, list_servers, get_invite, join_space};
use room::{create_room, list_rooms, send_message, list_messages};

#[derive(Default)]
pub struct AppState {
    pub node: TokioMutex<Option<Node>>,
    pub spaces: TokioMutex<Vec<Space>>,
    pub pending_call: Mutex<Option<oneshot::Sender<bool>>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            create_identity,
            login,
            list_identities,
            logout,
            delete_identity,
            create_server,
            list_servers,
            join_space,
            get_invite,
            create_room,
            list_rooms,
            send_message,
            list_messages,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
