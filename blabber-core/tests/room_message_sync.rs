//! Investigates a reported bug: after rejoining a space, rooms show up but
//! their messages never sync, neither history nor live. Runs two real
//! `Node`s in-process over real local Iroh endpoints.

use anyhow::Result;

mod common;
use common::make_node;

/// A room that exists before B joins is discovered by Space::sync_rooms's
/// bulk loop, which explicitly calls Room::watch for it. Sanity check that
/// this path works: history and live messages should both sync.
#[tokio::test]
async fn room_existing_before_join_syncs_messages() -> Result<()> {
    let (node_a, _dir_a) = make_node("PreexistingAlice").await?;
    let (node_b, _dir_b) = make_node("PreexistingBob").await?;

    let space_a = node_a.create_space("Preexisting Room Space").await?;
    let author_a = node_a.author.unwrap();
    let room_a = space_a.create_room(&node_a, author_a, "general").await?;
    room_a.send_message(author_a, "hello before B joins").await?;

    let space_b = node_b.join_space(space_a.create_invite().await?).await?;
    let blobs_b = node_b.blobs.clone().unwrap();

    let mut room_b = None;
    for _ in 0..50 {
        space_b.sync_rooms(&node_b, &blobs_b).await?;
        let rooms = space_b.rooms.lock().await;
        if let Some(r) = rooms.iter().find(|r| r.id == room_a.id) {
            room_b = Some(r.clone());
            break;
        }
        drop(rooms);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let room_b = room_b.expect("B never discovered the pre-existing room");

    let mut history = Vec::new();
    for _ in 0..50 {
        history = room_b.list_messages(blobs_b.clone()).await?;
        if !history.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(!history.is_empty(), "B should see the pre-existing message history");

    // live message sent after B joined
    room_a.send_message(author_a, "hello after B joined").await?;
    let mut saw_live = false;
    for _ in 0..50 {
        if room_b.list_messages(blobs_b.clone()).await?.len() >= 2 {
            saw_live = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(saw_live, "B should receive a live message sent after joining");

    Ok(())
}

/// A room created by A AFTER B has already joined the space is discovered
/// only through Space::try_apply_entry's live "room/" doc-watch path, not
/// sync_rooms's bulk loop. This is the reported bug: that path used to never
/// call Room::watch, so the room's messages doc was imported but never
/// subscribed - list_messages() still works (iroh-docs keeps an imported doc
/// live-synced regardless of subscription), but AppEvent::NewMessage, the
/// thing the frontend actually relies on to show messages while a room is
/// open, would never fire. Assert on the event directly, not list_messages,
/// since that's the part that was actually broken.
#[tokio::test]
async fn room_created_after_join_syncs_messages() -> Result<()> {
    let (node_a, _dir_a) = make_node("LateRoomAlice").await?;
    let (node_b, _dir_b) = make_node("LateRoomBob").await?;

    let space_a = node_a.create_space("Late Room Space").await?;
    let space_b = node_b.join_space(space_a.create_invite().await?).await?;
    let blobs_b = node_b.blobs.clone().unwrap();

    // B is fully joined and has already done its initial sync (finds nothing yet)
    space_b.sync_rooms(&node_b, &blobs_b).await?;
    assert!(space_b.rooms.lock().await.is_empty());

    // subscribe before A creates the room so we don't miss the event
    let mut events_b = node_b.subscribe_events();

    // NOW A creates a room, after B already joined
    let author_a = node_a.author.unwrap();
    let room_a = space_a.create_room(&node_a, author_a, "late-room").await?;

    // B should discover the room via the live info-doc watcher
    let mut room_b = None;
    for _ in 0..50 {
        let rooms = space_b.rooms.lock().await;
        if let Some(r) = rooms.iter().find(|r| r.id == room_a.id) {
            room_b = Some(r.clone());
            break;
        }
        drop(rooms);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let room_b = room_b.expect("B never discovered the room created after joining");

    // does B receive a live AppEvent::NewMessage for a message sent after
    // discovering the room? This is what a room open in the UI actually
    // relies on - list_messages() alone would pass even with the bug present.
    room_a.send_message(author_a, "live message in the late room").await?;

    let saw_event = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            match events_b.recv().await {
                Ok(blabber_core::AppEvent::NewMessage { room_id, .. }) if room_id == room_a.id => {
                    return true;
                }
                Ok(_) => continue,
                Err(_) => return false,
            }
        }
    })
    .await
    .unwrap_or(false);

    assert!(
        saw_event,
        "B never received a NewMessage event for a room discovered after joining - \
         Room::watch was never called for it, so the frontend would never see a live \
         message while sitting in that room"
    );

    // list_messages should also reflect it (already covered by the event, but
    // this is what a fresh loadMessages() fetch after re-entering the room uses)
    let mut saw_in_history = false;
    for _ in 0..50 {
        if !room_b.list_messages(blobs_b.clone()).await?.is_empty() {
            saw_in_history = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(saw_in_history, "B should also see the message via list_messages");

    Ok(())
}
