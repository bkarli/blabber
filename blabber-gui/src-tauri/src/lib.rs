use blabber_core::channel::ActiveVoiceCall;
use blabber_core::node::Node;
use blabber_core::space::Space;

use std::sync::Mutex;
use tokio::sync::Mutex as TokioMutex;

#[path = "commands/login.rs"]
mod login;
use login::{create_identity, list_identities, login,logout};

#[path = "commands/voice_channel.rs"]
mod voice_channel;
use voice_channel::{hang_up, start_call, my_endpoint_id};

#[path = "commands/space.rs"]
mod space;
use space::{create_server, list_servers};



#[path = "commands/room.rs"]
mod room;
use room::{create_room, list_rooms,
};

#[derive(Default)]
pub struct AppState {
    pub node: TokioMutex<Option<Node>>,
    pub active_call: Mutex<Option<ActiveVoiceCall>>,
    pub spaces: TokioMutex<Vec<Space>>,
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
            start_call,
            hang_up,
            create_server,
            my_endpoint_id,
            list_servers,
            create_room,
            list_rooms
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
