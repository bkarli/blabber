use blabber_core::channel::MeshActiveCall;
use blabber_core::node::Node;
use blabber_core::space::Space;
use std::sync::Mutex;
use tokio::sync::Mutex as TokioMutex;
use uuid::Uuid;

mod login;
mod space;
mod room;
mod call_room;
mod audio;
mod event_bridge;
mod result_ext;

use login::{create_identity, list_identities, login, logout, delete_identity};
use room::{create_room, list_rooms, send_message, list_messages, get_my_author_id, send_image, my_endpoint_id, send_file, get_media};
use space::{create_server, list_servers, get_invite, get_relay_invite, join_space, leave_space, list_members, list_connection_types};
use call_room::{create_call_room, list_call_rooms, join_call_room, leave_call_room, list_call_participants, set_muted};
use audio::{list_audio_devices, set_input_device, set_output_device, play_sound_effect};

#[derive(Default)]
pub struct AppState {
    pub node: TokioMutex<Option<Node>>,
    pub spaces: TokioMutex<Vec<Space>>,
    /// the mesh call this client is currently in, if any: (room id, live call handle)
    pub active_call_room: Mutex<Option<(Uuid, MeshActiveCall)>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
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
            leave_space,
            get_invite,
            get_relay_invite,
            create_room,
            list_rooms,
            send_message,
            send_image,
            send_file,
            get_media,
            list_messages,
            get_my_author_id,
            my_endpoint_id,
            list_members,
            list_connection_types,
            create_call_room,
            list_call_rooms,
            join_call_room,
            leave_call_room,
            list_call_participants,
            set_muted,
            list_audio_devices,
            set_input_device,
            set_output_device,
            play_sound_effect,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
