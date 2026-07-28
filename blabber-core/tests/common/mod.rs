//! Shared helpers for blabber-core's integration tests. Every test here runs
//! real `Node`s in-process over real local Iroh endpoints (no mocking of
//! networking/sync), so these helpers are all about spinning nodes up and
//! polling for eventually-consistent state to settle.

#![allow(dead_code)] // not every test file uses every helper

use anyhow::Result;
use blabber_core::call_rooms::CallRoom;
use blabber_core::channel::MeshActiveCall;
use blabber_core::room::{Message, MessageContent};
use blabber_core::{Identity, Node};
use iroh_docs::AuthorId;
use tempfile::TempDir;
use uuid::Uuid;

/// Extracts the text of a Text-content message, for convenient assertions
/// (`assert!(history.iter().any(|m| message_text(m) == Some("hi")))`).
pub fn message_text(message: &Message) -> Option<&str> {
    match &message.content {
        MessageContent::Text { text } => Some(text.as_str()),
        _ => None,
    }
}

/// Boots a fully-initialized Node (endpoint, gossip, blobs, docs, router,
/// author) in a fresh temp directory, ready to create/join spaces.
pub async fn make_node(name: &str) -> Result<(Node, TempDir)> {
    let dir = tempfile::tempdir()?;
    let mut node = Node::new(Identity::new(name));
    node.create_endpoint().await?;
    node.create_blobs(&dir.path().to_path_buf()).await?;
    node.create_gossip().await?;
    node.create_docs_engine(&dir.path().to_path_buf()).await?;
    node.create_router().await?;
    node.create_author().await?;
    Ok((node, dir))
}

/// Polls a synchronous predicate until it's true or the timeout elapses.
/// For checking in-memory state that doesn't itself require an `.await`
/// (e.g. `mesh.connection_count()`, `Arc<Mutex<..>>::try_lock`-free reads).
pub async fn wait_until<F>(timeout_ms: u64, step_ms: u64, mut check: F) -> bool
where
    F: FnMut() -> bool,
{
    let attempts = timeout_ms / step_ms;
    for _ in 0..attempts {
        if check() {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(step_ms)).await;
    }
    check()
}

/// Same as `wait_until`, but for a predicate that itself needs to `.await`
/// (e.g. re-running `list_members`/`list_messages`/`sync_rooms` each poll).
/// Collapses the "loop { do the async thing; check; sleep }" pattern that
/// otherwise gets hand-rolled at every call site.
pub async fn wait_until_async<F, Fut>(timeout_ms: u64, step_ms: u64, mut check: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let attempts = timeout_ms / step_ms;
    for _ in 0..attempts {
        if check().await {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(step_ms)).await;
    }
    check().await
}

/// Mirrors blabber-app/src-tauri/src/call_room.rs's `leave_call_room` Tauri
/// command exactly: hang up the mesh call, forget the room's mesh-routing
/// state, and publish the departure so other members' rosters update.
/// blabber-core has no single method for this - it's Tauri-layer glue over
/// public Node/CallRoom pieces - so tests replicate it here.
pub async fn leave_call_room(
    node: &Node,
    room_id: Uuid,
    call: MeshActiveCall,
    room: &CallRoom,
    author: AuthorId,
    my_id: String,
) -> Result<()> {
    call.hang_up();
    node.active_call_rooms.lock().unwrap().remove(&room_id);
    node.room_spaces.lock().unwrap().remove(&room_id);
    room.set_membership(author, my_id, false).await?;
    Ok(())
}
