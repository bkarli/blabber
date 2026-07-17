use tauri::{AppHandle, Emitter};
use tokio::sync::broadcast;
use blabber_core::node::Node;

pub fn spawn_event_bridge(app_handle: AppHandle, node: &Node) {
    let mut rx = node.subscribe_events();

    tauri::async_runtime::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    println!("new event d");
                    match app_handle.emit("app-event", event) {
                        Ok(()) => println!("emit succeeded"),
                        Err(e) => eprintln!("emit FAILED: {e}"),
                    }

                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    eprintln!("event bridge lagged, skipped {n} events");
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}
