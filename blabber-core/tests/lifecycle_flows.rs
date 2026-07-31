//! Extensive coverage for realistic multi-step program flows: join -> chat ->
//! call -> leave -> rejoin -> chat again, and variations with more peers,
//! more rooms, and out-of-order departures/rejoins/blob-heavy content. Runs
//! real `Node`s in-process over real local Iroh endpoints - no mocking of
//! networking, gossip, doc sync, or blob storage - so these exercise the
//! actual connection-establishment and content-sync machinery end to end,
//! including component discovery (rooms/call rooms/members) and their state
//! after each transition.

use anyhow::Result;

mod common;
use common::{leave_call_room, make_node, message_text, wait_until, wait_until_async};

/// The full narrative in one continuous session: join, chat, start and join
/// a call, leave the call, leave the space entirely, rejoin the space, prove
/// history survived, chat again (must arrive live, not just via history),
/// and rejoin the call too.
#[tokio::test]
async fn full_cycle_join_chat_call_leave_rejoin_chat() -> Result<()> {
    let (node_a, _dir_a) = make_node("CycleAlice").await?;
    let (node_b, dir_b) = make_node("CycleBob").await?;

    let blobs_a = node_a.blobs.clone().unwrap();
    let blobs_b = node_b.blobs.clone().unwrap();
    let id_b = node_b.endpoint.as_ref().unwrap().id().to_string();

    // --- join ---
    let space_a = node_a.create_space("Cycle Space").await?;
    let author_a = space_a.author().unwrap();
    let room_a = space_a.create_room(&node_a, author_a, "general").await?;

    let space_b = node_b.join_space(space_a.create_invite().await?).await?;
    let author_b = space_b.author().unwrap();

    assert!(
        wait_until_async(3000, 100, || async {
            space_a
                .list_members(&blobs_a)
                .await
                .unwrap_or_default()
                .iter()
                .any(|m| m.endpoint_id == id_b)
        })
        .await,
        "A should discover B as a member right after joining"
    );

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
    let room_b = room_b.expect("B should discover the pre-existing room on join");

    // --- chat ---
    room_b.send_message(author_b, "hi from B").await?;
    assert!(
        wait_until_async(3000, 100, || async {
            room_a
                .list_messages(blobs_a.clone())
                .await
                .unwrap_or_default()
                .iter()
                .any(|m| message_text(m) == Some("hi from B"))
        })
        .await,
        "A should receive B's chat message"
    );


    let call_room_a = space_a.create_call_room(&node_a, author_a, "voice").await?;

    let mut call_room_b = None;
    for _ in 0..50 {
        space_b.sync_call_rooms(&node_b, &blobs_b).await?;
        let rooms = space_b.call_rooms.lock().await;
        if let Some(r) = rooms.iter().find(|r| r.id == call_room_a.id) {
            call_room_b = Some(r.clone());
            break;
        }
        drop(rooms);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let call_room_b = call_room_b.expect("B should discover the call room");

    let id_a = node_a.endpoint.as_ref().unwrap().id().to_string();
    let (_call_a, mesh_a) = node_a.join_call_room(space_a.id(), &call_room_a).await?;

    // B must see A's active-membership entry sync into its own call_log
    // replica before dialing, or it has no known participant to connect to
    assert!(
        wait_until_async(3000, 100, || async {
            call_room_b.list_active_members(blobs_b.clone()).await.unwrap_or_default().contains(&id_a)
        })
        .await,
        "B should see A's active call membership before joining"
    );
    let (call_b, mesh_b) = node_b.join_call_room(space_b.id(), &call_room_b).await?;

    assert!(
        wait_until(3000, 100, || mesh_a.connection_count() == 1 && mesh_b.connection_count() == 1).await,
        "call mesh should connect both directions"
    );

    // --- leave (the call, not the space) ---
    leave_call_room(&node_b, call_room_b.id, call_b, &call_room_b, author_b, id_b.clone()).await?;
    assert!(
        wait_until(3000, 100, || mesh_a.connection_count() == 0).await,
        "A should observe B leaving the call"
    );

    // --- leave (the space) ---
    let spaces_root = dir_b.path().join("spaces");
    node_b.leave_space(space_b.id(), spaces_root).await?;
    assert!(
        wait_until_async(3000, 100, || async {
            !space_a
                .list_members(&blobs_a)
                .await
                .unwrap_or_default()
                .iter()
                .any(|m| m.endpoint_id == id_b)
        })
        .await,
        "A should observe B leaving the space"
    );

    // --- rejoin ---
    let space_b2 = node_b.join_space(space_a.create_invite().await?).await?;

    let mut room_b2 = None;
    for _ in 0..50 {
        space_b2.sync_rooms(&node_b, &blobs_b).await?;
        let rooms = space_b2.rooms.lock().await;
        if let Some(r) = rooms.iter().find(|r| r.id == room_a.id) {
            room_b2 = Some(r.clone());
            break;
        }
        drop(rooms);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let room_b2 = room_b2.expect("B should rediscover the room after rejoining");

    let mut call_room_b2 = None;
    for _ in 0..50 {
        space_b2.sync_call_rooms(&node_b, &blobs_b).await?;
        let rooms = space_b2.call_rooms.lock().await;
        if let Some(r) = rooms.iter().find(|r| r.id == call_room_a.id) {
            call_room_b2 = Some(r.clone());
            break;
        }
        drop(rooms);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let call_room_b2 = call_room_b2.expect("B should rediscover the call room after rejoining");

    // history must survive the leave/rejoin cycle
    let history = room_b2.list_messages(blobs_b.clone()).await?;
    assert!(
        history.iter().any(|m| message_text(m) == Some("hi from B")),
        "B should still see the original message history after rejoining"
    );

    // --- chat again: must arrive live, not just be fetchable via history ---
    let mut events_a = node_a.subscribe_events();
    room_b2.send_message(author_b, "hi again after rejoining").await?;
    let saw_event = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            match events_a.recv().await {
                Ok(blabber_core::AppEvent::NewMessage { message, .. })
                    if message_text(&message) == Some("hi again after rejoining") =>
                {
                    return true;
                }
                Ok(_) => continue,
                Err(_) => return false,
            }
        }
    })
    .await
    .unwrap_or(false);
    assert!(saw_event, "A should receive B's post-rejoin message live");

    // --- rejoin the call too (A never left, so this exercises "someone
    // dials back into a call I'm still in") ---
    let (_call_b2, mesh_b2) = node_b.join_call_room(space_b2.id(), &call_room_b2).await?;
    assert!(
        wait_until(3000, 100, || mesh_a.connection_count() == 1 && mesh_b2.connection_count() == 1).await,
        "call mesh should reconnect after B rejoins"
    );

    Ok(())
}

/// Three peers, staggered: A creates, B joins and chats, C joins late (must
/// get full history - the blob/doc content sync path, not just presence),
/// B leaves (departure observed, but B's history stays), A chats while B is
/// away, B rejoins and must see everything including what happened while
/// gone, then B chats again and everyone still present must get it live.
#[tokio::test]
async fn three_peers_interleaved_join_chat_leave_rejoin() -> Result<()> {
    let (node_a, _dir_a) = make_node("TriAlice").await?;
    let (node_b, dir_b) = make_node("TriBob").await?;
    let (node_c, _dir_c) = make_node("TriCarol").await?;

    let blobs_a = node_a.blobs.clone().unwrap();
    let blobs_b = node_b.blobs.clone().unwrap();
    let blobs_c = node_c.blobs.clone().unwrap();
    let id_b = node_b.endpoint.as_ref().unwrap().id().to_string();

    let space_a = node_a.create_space("Tri Space").await?;
    let author_a = space_a.author().unwrap();
    let room_a = space_a.create_room(&node_a, author_a, "general").await?;
    room_a.send_message(author_a, "msg1 from A").await?;

    let space_b = node_b.join_space(space_a.create_invite().await?).await?;
    let author_b = space_b.author().unwrap();
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
    let room_b = room_b.expect("B should discover the room");
    room_b.send_message(author_b, "msg2 from B").await?;

    // C joins late, after both messages already exist
    let space_c = node_c.join_space(space_a.create_invite().await?).await?;
    let mut room_c = None;
    for _ in 0..50 {
        space_c.sync_rooms(&node_c, &blobs_c).await?;
        let rooms = space_c.rooms.lock().await;
        if let Some(r) = rooms.iter().find(|r| r.id == room_a.id) {
            room_c = Some(r.clone());
            break;
        }
        drop(rooms);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let room_c = room_c.expect("C should discover the room even joining late");

    assert!(
        wait_until_async(3000, 100, || async {
            let history = room_c.list_messages(blobs_c.clone()).await.unwrap_or_default();
            history.iter().any(|m| message_text(m) == Some("msg1 from A"))
                && history.iter().any(|m| message_text(m) == Some("msg2 from B"))
        })
        .await,
        "C should see full history from before it joined"
    );
    assert!(
        wait_until_async(3000, 100, || async {
            space_c
                .list_members(&blobs_c)
                .await
                .unwrap_or_default()
                .iter()
                .any(|m| m.endpoint_id == id_b)
        })
        .await,
        "C should see B as a member"
    );

    // B leaves
    let spaces_root = dir_b.path().join("spaces");
    node_b.leave_space(space_b.id(), spaces_root).await?;

    assert!(
        wait_until_async(3000, 100, || async {
            !space_a.list_members(&blobs_a).await.unwrap_or_default().iter().any(|m| m.endpoint_id == id_b)
                && !space_c.list_members(&blobs_c).await.unwrap_or_default().iter().any(|m| m.endpoint_id == id_b)
        })
        .await,
        "A and C should both observe B leaving"
    );

    // B's earlier message must still be visible to everyone else
    let still_visible = room_a.list_messages(blobs_a.clone()).await?;
    assert!(still_visible.iter().any(|m| message_text(m) == Some("msg2 from B")), "leaving must not erase message history");

    // A chats while B is away
    room_a.send_message(author_a, "msg3 from A while B is away").await?;

    // B rejoins
    let space_b2 = node_b.join_space(space_a.create_invite().await?).await?;
    let mut room_b2 = None;
    for _ in 0..50 {
        space_b2.sync_rooms(&node_b, &blobs_b).await?;
        let rooms = space_b2.rooms.lock().await;
        if let Some(r) = rooms.iter().find(|r| r.id == room_a.id) {
            room_b2 = Some(r.clone());
            break;
        }
        drop(rooms);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let room_b2 = room_b2.expect("B should rediscover the room after rejoining");

    assert!(
        wait_until_async(3000, 100, || async {
            space_a.list_members(&blobs_a).await.unwrap_or_default().iter().any(|m| m.endpoint_id == id_b)
                && space_c.list_members(&blobs_c).await.unwrap_or_default().iter().any(|m| m.endpoint_id == id_b)
        })
        .await,
        "A and C should both see B reappear as a member"
    );

    assert!(
        wait_until_async(3000, 100, || async {
            let history = room_b2.list_messages(blobs_b.clone()).await.unwrap_or_default();
            ["msg1 from A", "msg2 from B", "msg3 from A while B is away"]
                .iter()
                .all(|text| history.iter().any(|m| message_text(m) == Some(text)))
        })
        .await,
        "B should see everything that happened while away, including messages sent during the gap"
    );

    // B chats again - both A and C must get it live
    let mut events_a = node_a.subscribe_events();
    let mut events_c = node_c.subscribe_events();
    room_b2.send_message(author_b, "msg4 from B after rejoining").await?;

    for (label, events) in [("A", &mut events_a), ("C", &mut events_c)] {
        let saw = tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                match events.recv().await {
                    Ok(blabber_core::AppEvent::NewMessage { message, .. })
                        if message_text(&message) == Some("msg4 from B after rejoining") =>
                    {
                        return true;
                    }
                    Ok(_) => continue,
                    Err(_) => return false,
                }
            }
        })
        .await
        .unwrap_or(false);
        assert!(saw, "{label} should receive B's post-rejoin message live");
    }

    Ok(())
}

/// Blob content (not just doc metadata) must sync correctly to a peer
/// joining after the content was created, and to a second peer joining even
/// later - exercises the actual content-addressed blob transfer, not just
/// small message payloads.
#[tokio::test]
async fn image_blob_content_syncs_correctly_to_late_joiners() -> Result<()> {
    let (node_a, _dir_a) = make_node("BlobAlice").await?;
    let (node_b, _dir_b) = make_node("BlobBob").await?;
    let (node_c, _dir_c) = make_node("BlobCarol").await?;

    let space_a = node_a.create_space("Blob Space").await?;
    let author_a = space_a.author().unwrap();
    let room_a = space_a.create_room(&node_a, author_a, "images").await?;

    // a few KB of deterministic pseudo-random bytes, big enough to be a
    // meaningfully-sized blob rather than a trivial payload. Note this is
    // large/random enough that image::load_from_memory in generate_thumbnail
    // will fail to decode it as a real image format - that's fine, the
    // thumbnail generation error just means send_image returns an error in
    // that case, so we use bytes that at least look like nothing decodable
    // is required for the full media round trip, which is what this test
    // actually exercises via store_media/get_media.
    let image_bytes: Vec<u8> = (0..4000u32).map(|i| (i % 251) as u8).collect();
    let media_key_a = room_a.store_media(author_a, &image_bytes).await?;
    room_a
        .send_content(author_a, blabber_core::room::MessageContent::File {
            filename: "photo.png".to_string(),
            mime: "image/png".to_string(),
            size: image_bytes.len() as u64,
            media_key: media_key_a.clone(),
        })
        .await?;

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
    let room_b = room_b.expect("B should discover the room");

    assert!(
        wait_until_async(5000, 100, || {
            let room_b = &room_b;
            let blobs_b = blobs_b.clone();
            let media_key_a = media_key_a.clone();
            async move {
                room_b
                    .get_media(&media_key_a, blobs_b)
                    .await
                    .ok()
                    .flatten()
                    .is_some()
            }
        })
        .await,
        "B should eventually be able to fetch the media blob"
    );
    let media_b = room_b
        .get_media(&media_key_a, blobs_b.clone())
        .await?
        .expect("B should have the media");
    assert_eq!(media_b, image_bytes, "B's media bytes must match exactly, not be truncated/corrupted");

    // C joins even later, after B already synced it
    let space_c = node_c.join_space(space_a.create_invite().await?).await?;
    let blobs_c = node_c.blobs.clone().unwrap();
    let mut room_c = None;
    for _ in 0..50 {
        space_c.sync_rooms(&node_c, &blobs_c).await?;
        let rooms = space_c.rooms.lock().await;
        if let Some(r) = rooms.iter().find(|r| r.id == room_a.id) {
            room_c = Some(r.clone());
            break;
        }
        drop(rooms);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let room_c = room_c.expect("C should discover the room");

    assert!(
        wait_until_async(5000, 100, || {
            let room_c = &room_c;
            let blobs_c = blobs_c.clone();
            let media_key_a = media_key_a.clone();
            async move {
                room_c
                    .get_media(&media_key_a, blobs_c)
                    .await
                    .ok()
                    .flatten()
                    .is_some()
            }
        })
        .await,
        "C should also eventually be able to fetch the media blob"
    );
    let media_c = room_c
        .get_media(&media_key_a, blobs_c.clone())
        .await?
        .expect("C should have the media");
    assert_eq!(media_c, image_bytes, "C's media bytes must also match exactly");

    Ok(())
}

/// A call room's mesh connection must correctly tear down and re-establish
/// across repeated leave/rejoin cycles of the *same* call room (not the
/// whole space), without accumulating stale/duplicate connections.
#[tokio::test]
async fn call_room_reconnects_after_leaving_and_rejoining_same_call() -> Result<()> {
    let (node_a, _dir_a) = make_node("ReconnectAlice").await?;
    let (node_b, _dir_b) = make_node("ReconnectBob").await?;

    let id_b = node_b.endpoint.as_ref().unwrap().id().to_string();

    let space_a = node_a.create_space("Reconnect Space").await?;
    let author_a = space_a.author().unwrap();
    let call_room_a = space_a.create_call_room(&node_a, author_a, "voice").await?;

    let space_b = node_b.join_space(space_a.create_invite().await?).await?;
    let author_b = space_b.author().unwrap();
    let blobs_b = node_b.blobs.clone().unwrap();
    let mut call_room_b = None;
    for _ in 0..50 {
        space_b.sync_call_rooms(&node_b, &blobs_b).await?;
        let rooms = space_b.call_rooms.lock().await;
        if let Some(r) = rooms.iter().find(|r| r.id == call_room_a.id) {
            call_room_b = Some(r.clone());
            break;
        }
        drop(rooms);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let call_room_b = call_room_b.expect("B should discover the call room");

    let id_a = node_a.endpoint.as_ref().unwrap().id().to_string();
    let (_call_a, mesh_a) = node_a.join_call_room(space_a.id(), &call_room_a).await?;
    assert!(
        wait_until_async(3000, 100, || async {
            call_room_b.list_active_members(blobs_b.clone()).await.unwrap_or_default().contains(&id_a)
        })
        .await,
        "B should see A's active call membership before joining"
    );

    // do two full leave+rejoin cycles of the same call room
    for cycle in 1..=2 {
        let mut events_a = node_a.subscribe_events();
        let (call_b, mesh_b) = node_b.join_call_room(space_b.id(), &call_room_b).await?;

        assert!(
            wait_until(3000, 100, || mesh_a.connection_count() == 1 && mesh_b.connection_count() == 1).await,
            "cycle {cycle}: call mesh should connect both directions"
        );

        let saw_join = tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                match events_a.recv().await {
                    Ok(blabber_core::AppEvent::NewCallParticipant { endpoint_id, .. }) if endpoint_id == id_b => return true,
                    Ok(_) => continue,
                    Err(_) => return false,
                }
            }
        })
        .await
        .unwrap_or(false);
        assert!(saw_join, "cycle {cycle}: A should see a NewCallParticipant event for B joining");

        leave_call_room(&node_b, call_room_b.id, call_b, &call_room_b, author_b, id_b.clone()).await?;

        assert!(
            wait_until(3000, 100, || mesh_a.connection_count() == 0).await,
            "cycle {cycle}: A should observe B leaving, with no stale connection left behind"
        );
    }

    Ok(())
}

/// After a rich flow (multiple chat rooms, multiple call rooms, messages in
/// each), every piece of state a rejoining peer needs must be correctly
/// discoverable and correct - not just "something shows up", but the right
/// counts, the right names, the right per-room message isolation, and the
/// right member fields.
#[tokio::test]
async fn full_component_state_audit_after_complex_flow() -> Result<()> {
    let (node_a, _dir_a) = make_node("AuditAlice").await?;
    let (node_b, _dir_b) = make_node("AuditBob").await?;

    let id_a = node_a.endpoint.as_ref().unwrap().id().to_string();
    let id_b = node_b.endpoint.as_ref().unwrap().id().to_string();

    let space_a = node_a.create_space("Audit Space").await?;
    let author_a = space_a.author().unwrap();
    let general_a = space_a.create_room(&node_a, author_a, "general").await?;
    let random_a = space_a.create_room(&node_a, author_a, "random").await?;
    let voice_a = space_a.create_call_room(&node_a, author_a, "voice").await?;

    general_a.send_message(author_a, "general message").await?;
    random_a.send_message(author_a, "random message").await?;

    let space_b = node_b.join_space(space_a.create_invite().await?).await?;
    let blobs_b = node_b.blobs.clone().unwrap();

    assert!(
        wait_until_async(3000, 100, || async {
            space_b.sync_rooms(&node_b, &blobs_b).await.is_ok()
                && space_b.sync_call_rooms(&node_b, &blobs_b).await.is_ok()
                && space_b.rooms.lock().await.len() == 2
                && space_b.call_rooms.lock().await.len() == 1
        })
        .await,
        "B should discover exactly 2 rooms and 1 call room"
    );

    // component: node-level space bookkeeping
    assert_eq!(node_b.spaces.lock().await.len(), 1, "B's node should track exactly one space");

    // component: room names, both present, no extras
    let room_names: std::collections::HashSet<String> =
        space_b.rooms.lock().await.iter().map(|r| r.name.clone()).collect();
    assert_eq!(
        room_names,
        ["general", "random"].into_iter().map(String::from).collect(),
        "B's room set should be exactly {{general, random}}"
    );

    // component: call room name
    let call_room_names: Vec<String> = space_b.call_rooms.lock().await.iter().map(|r| r.name.clone()).collect();
    assert_eq!(call_room_names, vec!["voice".to_string()]);

    // component: members, both present with correct fields
    let members = space_b.list_members(&blobs_b).await?;
    assert_eq!(members.len(), 2, "exactly A and B should be members");
    let member_a = members.iter().find(|m| m.endpoint_id == id_a).expect("A should be a member");
    assert_eq!(member_a.display_name, "AuditAlice");
    let member_b = members.iter().find(|m| m.endpoint_id == id_b).expect("B should be a member");
    assert_eq!(member_b.display_name, "AuditBob");

    // component: per-room message isolation - a message sent in one room
    // must never leak into another room's history
    let general_b = space_b.rooms.lock().await.iter().find(|r| r.name == "general").unwrap().clone();
    let random_b = space_b.rooms.lock().await.iter().find(|r| r.name == "random").unwrap().clone();

    assert!(
        wait_until_async(3000, 100, || async {
            !general_b.list_messages(blobs_b.clone()).await.unwrap_or_default().is_empty()
                && !random_b.list_messages(blobs_b.clone()).await.unwrap_or_default().is_empty()
        })
        .await,
        "both rooms' messages should sync"
    );

    let general_history = general_b.list_messages(blobs_b.clone()).await?;
    assert!(general_history.iter().any(|m| message_text(m) == Some("general message")));
    assert!(
        !general_history.iter().any(|m| message_text(m) == Some("random message")),
        "random's message must not leak into general's history"
    );

    let random_history = random_b.list_messages(blobs_b.clone()).await?;
    assert!(random_history.iter().any(|m| message_text(m) == Some("random message")));
    assert!(
        !random_history.iter().any(|m| message_text(m) == Some("general message")),
        "general's message must not leak into random's history"
    );

    // component: call room active membership, correct on both sides once joined
    let voice_b = space_b.call_rooms.lock().await.first().unwrap().clone();
    let (_call_a, _mesh_a) = node_a.join_call_room(space_a.id(), &voice_a).await?;
    let (_call_b, _mesh_b) = node_b.join_call_room(space_b.id(), &voice_b).await?;

    assert!(
        wait_until_async(3000, 100, || async {
            let members_seen = voice_a.list_active_members(node_a.blobs.clone().unwrap()).await.unwrap_or_default();
            members_seen.contains(&id_a) && members_seen.contains(&id_b)
        })
        .await,
        "call room should report both A and B as active members"
    );

    Ok(())
}

/// A peer leaves and rejoins a space that already has several rooms and call
/// rooms. Mirrors the real app's actual join flow (blabber-app/src-tauri's
/// `join_space` command always does one bulk `sync_rooms`/`sync_call_rooms`
/// pass immediately after joining - unlike relying on live discovery alone,
/// that bulk query reads current doc state directly and isn't subject to
/// any import/subscribe timing race, so it's the reliable way to catch a
/// batch of pre-existing entries). After that initial resync, anything
/// created *afterward* - while B is fully caught up and steady-state - must
/// still be discovered purely through the live watcher, extending the
/// single-room regression in room_message_sync.rs to a multi-room and
/// multi-call-room scenario.
#[tokio::test]
async fn multiple_rooms_and_call_rooms_all_discovered_after_rejoin() -> Result<()> {
    let (node_a, _dir_a) = make_node("MultiAlice").await?;
    let (node_b, dir_b) = make_node("MultiBob").await?;

    let space_a = node_a.create_space("Multi Space").await?;
    let author_a = space_a.author().unwrap();
    let room1_a = space_a.create_room(&node_a, author_a, "room-one").await?;
    let room2_a = space_a.create_room(&node_a, author_a, "room-two").await?;
    let call1_a = space_a.create_call_room(&node_a, author_a, "call-one").await?;

    let space_b = node_b.join_space(space_a.create_invite().await?).await?;
    let blobs_b = node_b.blobs.clone().unwrap();
    space_b.sync_rooms(&node_b, &blobs_b).await?;
    space_b.sync_call_rooms(&node_b, &blobs_b).await?;

    node_b.leave_space(space_b.id(), dir_b.path().join("spaces")).await?;

    // B rejoins and does the same one-shot bulk resync the Tauri join_space
    // command always does - this is what actually protects against a batch
    // of pre-existing entries racing the live-event subscription.
    let space_b2 = node_b.join_space(space_a.create_invite().await?).await?;
    assert!(
        wait_until_async(3000, 100, || async {
            space_b2.sync_rooms(&node_b, &blobs_b).await.is_ok()
                && space_b2.sync_call_rooms(&node_b, &blobs_b).await.is_ok()
                && space_b2.rooms.lock().await.len() == 2
                && space_b2.call_rooms.lock().await.len() == 1
        })
        .await,
        "B's initial resync after rejoining should find both pre-existing rooms and the call room"
    );

    // NOW, with B fully caught up and steady-state, A creates more of each -
    // these can ONLY be found through the live watcher, no bulk call follows.
    let room3_a = space_a.create_room(&node_a, author_a, "room-three").await?;
    let call2_a = space_a.create_call_room(&node_a, author_a, "call-two").await?;

    assert!(
        wait_until_async(3000, 100, || async {
            space_b2.rooms.lock().await.len() == 3 && space_b2.call_rooms.lock().await.len() == 2
        })
        .await,
        "B should discover the room and call room created after it was already caught up, purely via live gossip"
    );

    for (room_a, expected_name) in [(&room1_a, "room-one"), (&room2_a, "room-two"), (&room3_a, "room-three")] {
        let room_b = {
            let rooms = space_b2.rooms.lock().await;
            rooms.iter().find(|r| r.id == room_a.id).cloned()
        }
        .unwrap_or_else(|| panic!("B should have discovered {expected_name}"));

        let marker = format!("live message in {expected_name}");
        room_a.send_message(author_a, marker.clone()).await?;

        assert!(
            wait_until_async(3000, 100, || {
                let room_b = &room_b;
                let blobs_b = blobs_b.clone();
                let marker = marker.clone();
                async move {
                    room_b
                        .list_messages(blobs_b)
                        .await
                        .unwrap_or_default()
                        .iter()
                        .any(|m| message_text(m) == Some(marker.as_str()))
                }
            })
            .await,
            "{expected_name}: B should receive a live message, proving Room::watch was wired up for it - whether \
             discovered via the initial bulk resync or the live watcher afterward"
        );
    }

    let call_ids: std::collections::HashSet<uuid::Uuid> = space_b2.call_rooms.lock().await.iter().map(|r| r.id).collect();
    assert!(call_ids.contains(&call1_a.id) && call_ids.contains(&call2_a.id), "B should have discovered both call rooms");

    Ok(())
}
