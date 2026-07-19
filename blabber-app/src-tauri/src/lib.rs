use blabber_core::channel::MeshActiveCall;
use blabber_core::node::Node;
use blabber_core::space::Space;
use std::sync::Mutex;
use tokio::sync::Mutex as TokioMutex;
use tokio::sync::oneshot;
use uuid::Uuid;

mod login;
mod space;
mod room;
mod voice_channel;
mod call_room;
mod event_bridge;

use login::{create_identity, list_identities, login, logout, delete_identity};
use room::{create_room, list_rooms, send_message, list_messages,get_my_author_id};
use space::{create_server, list_servers, get_invite, join_space,list_members};
use call_room::{create_call_room, list_call_rooms, join_call_room, leave_call_room, list_call_participants};

#[derive(Default)]
pub struct AppState {
    pub node: TokioMutex<Option<Node>>,
    pub spaces: TokioMutex<Vec<Space>>,
    pub pending_call: Mutex<Option<oneshot::Sender<bool>>>,
    pub active_call_room: Mutex<Option<(Uuid, MeshActiveCall)>>,
}

#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
fn enable_webview_media_devices(app: &tauri::App) {
    use tauri::Manager;
    use webkit2gtk::{PermissionRequestExt, SettingsExt, WebViewExt};

    let Some(window) = app.get_webview_window("main") else { return };
    let _ = window.with_webview(|webview| {
        let wv = webview.inner();
        if let Some(settings) = wv.settings() {
            settings.set_enable_media_stream(true);
            settings.set_enable_mediasource(true);
        }
        wv.connect_permission_request(|_wv, request| {
            request.allow();
            true
        });
    });
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
)))]
fn enable_webview_media_devices(_app: &tauri::App) {}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .setup(|app| {
            enable_webview_media_devices(app);
            Ok(())
        })
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
            get_my_author_id,
            list_members,
            create_call_room,
            list_call_rooms,
            join_call_room,
            leave_call_room,
            list_call_participants,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
