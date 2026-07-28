//! End-to-end regression tests for the group call room flow: discovery via the
//! synced call-log doc, mesh connection formation, bidirectional audio datagram
//! delivery, and peer/room cleanup. These run two (or three) real `Node`s
//! in-process over real local Iroh endpoints, so they catch any regression in
//! the mesh/discovery/cleanup code that doesn't depend on real-world network
//! conditions (NAT traversal, path MTU, etc. can't be reproduced on one machine).

use anyhow::Result;
use blabber_core::{Identity, Node};
use tempfile::TempDir;
use uuid::Uuid;

async fn make_node(name: &str) -> Result<(Node, TempDir)> {
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

async fn wait_until<F>(timeout_ms: u64, step_ms: u64, mut check: F) -> bool
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

#[tokio::test]
async fn two_peer_mesh_connects_both_directions() -> Result<()> {
    let (node_a, _dir_a) = make_node("TestAlice").await?;
    let (node_b, _dir_b) = make_node("TestBob").await?;

    let room_id = Uuid::new_v4();
    let id_a = node_a.endpoint.as_ref().unwrap().id();
    let addr_a = node_a.endpoint.as_ref().unwrap().addr();
    let id_b = node_b.endpoint.as_ref().unwrap().id();

    // A joins first, with no known peers yet
    let (_call_a, mesh_a) = node_a.join_mesh(room_id, id_a.to_string(), vec![]).await?;

    // B joins second, dialing out to A directly (exactly what join_call_room does
    // for a newcomer once it has read the known participants from the call log)
    let (_call_b, mesh_b) = node_b
        .join_mesh(room_id, id_b.to_string(), vec![(id_a.to_string(), addr_a)])
        .await?;

    let connected = wait_until(3000, 100, || {
        mesh_a.connection_count() == 1 && mesh_b.connection_count() == 1
    })
    .await;

    assert!(
        connected,
        "expected both sides connected, got A={} B={}",
        mesh_a.connection_count(),
        mesh_b.connection_count()
    );
    assert_eq!(mesh_a.peer_ids(), vec![id_b.to_string()]);
    assert_eq!(mesh_b.peer_ids(), vec![id_a.to_string()]);

    Ok(())
}

#[tokio::test]
async fn three_peer_mesh_is_fully_connected() -> Result<()> {
    let (node_a, _dir_a) = make_node("TestAlice3").await?;
    let (node_b, _dir_b) = make_node("TestBob3").await?;
    let (node_c, _dir_c) = make_node("TestCarol3").await?;

    let room_id = Uuid::new_v4();
    let id_a = node_a.endpoint.as_ref().unwrap().id();
    let addr_a = node_a.endpoint.as_ref().unwrap().addr();
    let id_b = node_b.endpoint.as_ref().unwrap().id();
    let addr_b = node_b.endpoint.as_ref().unwrap().addr();
    let id_c = node_c.endpoint.as_ref().unwrap().id();

    let (_call_a, mesh_a) = node_a.join_mesh(room_id, id_a.to_string(), vec![]).await?;
    let (_call_b, mesh_b) = node_b
        .join_mesh(room_id, id_b.to_string(), vec![(id_a.to_string(), addr_a.clone())])
        .await?;
    let (_call_c, mesh_c) = node_c
        .join_mesh(
            room_id,
            id_c.to_string(),
            vec![(id_a.to_string(), addr_a), (id_b.to_string(), addr_b)],
        )
        .await?;

    let fully_meshed = wait_until(4000, 100, || {
        mesh_a.connection_count() == 2 && mesh_b.connection_count() == 2 && mesh_c.connection_count() == 2
    })
    .await;

    assert!(
        fully_meshed,
        "expected a fully connected triangle, got A={} B={} C={}",
        mesh_a.connection_count(),
        mesh_b.connection_count(),
        mesh_c.connection_count()
    );

    Ok(())
}

#[tokio::test]
async fn call_room_discovery_syncs_and_connects() -> Result<()> {
    let (node_a, _dir_a) = make_node("DiscAlice").await?;
    let (node_b, _dir_b) = make_node("DiscBob").await?;

    let blobs_b = node_b.blobs.clone().unwrap();

    let space_a = node_a.create_space("Test Space").await?;
    let invite = space_a.create_invite().await?;
    let space_b = node_b.join_space(invite).await?;

    let author_a = node_a.author.unwrap();
    let room_a = space_a.create_call_room(author_a, "Test Call").await?;

    let (_call_a, mesh_a) = node_a.join_call_room(space_a.id(), &room_a).await?;

    // B discovers the call room via space info doc sync
    let mut room_b = None;
    for _ in 0..100 {
        space_b.sync_call_rooms(&blobs_b).await?;
        let rooms = space_b.call_rooms.lock().await;
        if let Some(r) = rooms.iter().find(|r| r.id == room_a.id) {
            room_b = Some(r.clone());
            break;
        }
        drop(rooms);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let room_b = room_b.expect("B never discovered the call room via doc sync");

    // B sees A's membership entry via the call room's doc sync
    let id_a = node_a.endpoint.as_ref().unwrap().id().to_string();
    let mut saw_membership = false;
    for _ in 0..100 {
        if room_b.list_active_members(blobs_b.clone()).await?.contains(&id_a) {
            saw_membership = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(saw_membership, "B never saw A's membership entry via doc sync");

    let (_call_b, mesh_b) = node_b.join_call_room(space_b.id(), &room_b).await?;

    let connected = wait_until(3000, 100, || {
        mesh_a.connection_count() == 1 && mesh_b.connection_count() == 1
    })
    .await;
    assert!(
        connected,
        "expected mesh to form after discovery, got A={} B={}",
        mesh_a.connection_count(),
        mesh_b.connection_count()
    );

    Ok(())
}

#[tokio::test]
async fn audio_datagrams_flow_in_both_directions() -> Result<()> {
    let (node_a, _dir_a) = make_node("DatagramAlice").await?;
    let (node_b, _dir_b) = make_node("DatagramBob").await?;

    let room_id = Uuid::new_v4();
    let id_a = node_a.endpoint.as_ref().unwrap().id();
    let addr_a = node_a.endpoint.as_ref().unwrap().addr();
    let id_b = node_b.endpoint.as_ref().unwrap().id();

    let (_call_a, mesh_a) = node_a.join_mesh(room_id, id_a.to_string(), vec![]).await?;
    let (_call_b, mesh_b) = node_b
        .join_mesh(room_id, id_b.to_string(), vec![(id_a.to_string(), addr_a)])
        .await?;

    wait_until(3000, 100, || {
        mesh_a.connection_count() == 1 && mesh_b.connection_count() == 1
    })
    .await;

    let conn_a_to_b = mesh_a
        .connection_for(&id_b.to_string())
        .expect("A has no connection to B");
    conn_a_to_b.send_datagram(vec![1u8; 64].into())?;

    let conn_b_to_a = mesh_b
        .connection_for(&id_a.to_string())
        .expect("B has no connection to A");
    conn_b_to_a.send_datagram(vec![2u8; 64].into())?;

    let b_received = wait_until(2000, 50, || {
        mesh_b.buffered_sample_count(&id_a.to_string()).unwrap_or(0) > 0
    })
    .await;
    let a_received = wait_until(2000, 50, || {
        mesh_a.buffered_sample_count(&id_b.to_string()).unwrap_or(0) > 0
    })
    .await;

    assert!(b_received, "B never received A's datagram (A->B direction broken)");
    assert!(a_received, "A never received B's datagram (B->A direction broken)");

    Ok(())
}

#[tokio::test]
async fn disconnected_peer_is_cleaned_up_automatically() -> Result<()> {
    let (node_a, _dir_a) = make_node("CleanupAlice").await?;
    let (node_b, _dir_b) = make_node("CleanupBob").await?;

    let room_id = Uuid::new_v4();
    let id_a = node_a.endpoint.as_ref().unwrap().id();
    let addr_a = node_a.endpoint.as_ref().unwrap().addr();
    let id_b = node_b.endpoint.as_ref().unwrap().id();

    let (_call_a, mesh_a) = node_a.join_mesh(room_id, id_a.to_string(), vec![]).await?;
    let (_call_b, mesh_b) = node_b
        .join_mesh(room_id, id_b.to_string(), vec![(id_a.to_string(), addr_a)])
        .await?;

    wait_until(3000, 100, || {
        mesh_a.connection_count() == 1 && mesh_b.connection_count() == 1
    })
    .await;

    // B hangs up on A directly (simulates B leaving/crashing)
    let conn_b_to_a = mesh_b.connection_for(&id_a.to_string()).unwrap();
    conn_b_to_a.close(0u32.into(), b"test hangup");

    // A's reader task for B should observe the close, error out, and call
    // remove_peer on itself instead of leaving a dead connection registered
    let cleaned_up = wait_until(3000, 100, || mesh_a.connection_count() == 0).await;

    assert!(
        cleaned_up,
        "expected A to clean up B's dead connection, still has {} peers",
        mesh_a.connection_count()
    );

    Ok(())
}

#[tokio::test]
async fn left_participant_is_not_redialed_by_new_joiner() -> Result<()> {
    let (node_a, _dir_a) = make_node("LeaveAlice").await?;
    let (node_b, _dir_b) = make_node("LeaveBob").await?;
    let (node_c, _dir_c) = make_node("LeaveCarol").await?;

    let blobs_b = node_b.blobs.clone().unwrap();
    let blobs_c = node_c.blobs.clone().unwrap();

    let space_a = node_a.create_space("Leave Test Space").await?;
    let space_b = node_b.join_space(space_a.create_invite().await?).await?;
    let space_c = node_c.join_space(space_a.create_invite().await?).await?;

    let author_a = node_a.author.unwrap();
    let author_b = node_b.author.unwrap();
    let room_a = space_a.create_call_room(author_a, "Leave Test Call").await?;

    let (_call_a, mesh_a) = node_a.join_call_room(space_a.id(), &room_a).await?;

    let id_a = node_a.endpoint.as_ref().unwrap().id().to_string();
    let id_b = node_b.endpoint.as_ref().unwrap().id().to_string();

    // B discovers the room and A's membership, then joins
    let mut room_b = None;
    for _ in 0..100 {
        space_b.sync_call_rooms(&blobs_b).await?;
        let rooms = space_b.call_rooms.lock().await;
        if let Some(r) = rooms.iter().find(|r| r.id == room_a.id) {
            room_b = Some(r.clone());
            break;
        }
        drop(rooms);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let room_b = room_b.expect("B never discovered the call room");
    for _ in 0..100 {
        if room_b.list_active_members(blobs_b.clone()).await?.contains(&id_a) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let (_call_b, mesh_b) = node_b.join_call_room(space_b.id(), &room_b).await?;

    let connected = wait_until(3000, 100, || {
        mesh_a.connection_count() == 1 && mesh_b.connection_count() == 1
    })
    .await;
    assert!(connected, "expected A and B to connect before B leaves");

    // B leaves - marks itself inactive instead of just disappearing
    room_b.set_membership(author_b, id_b.clone(), false).await?;

    // C joins afterwards and should discover only A as an active member
    let mut room_c = None;
    for _ in 0..100 {
        space_c.sync_call_rooms(&blobs_c).await?;
        let rooms = space_c.call_rooms.lock().await;
        if let Some(r) = rooms.iter().find(|r| r.id == room_a.id) {
            room_c = Some(r.clone());
            break;
        }
        drop(rooms);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let room_c = room_c.expect("C never discovered the call room");

    let mut members_seen = Vec::new();
    for _ in 0..100 {
        members_seen = room_c.list_active_members(blobs_c.clone()).await?;
        if members_seen.contains(&id_a) && !members_seen.contains(&id_b) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(members_seen.contains(&id_a), "C should see A as active");
    assert!(
        !members_seen.contains(&id_b),
        "C should NOT see B as active after B left - stale peers must not be retried forever"
    );

    let (_call_c, mesh_c) = node_c.join_call_room(space_c.id(), &room_c).await?;
    wait_until(3000, 100, || mesh_c.connection_count() >= 1).await;

    assert_eq!(
        mesh_c.connection_count(),
        1,
        "C should only connect to A, not dial the departed B"
    );
    assert_eq!(mesh_c.peer_ids(), vec![id_a]);

    Ok(())
}

#[tokio::test]
async fn accept_side_emits_new_call_participant_event() -> Result<()> {
    let (node_a, _dir_a) = make_node("EventAlice").await?;
    let (node_b, _dir_b) = make_node("EventBob").await?;

    let blobs_b = node_b.blobs.clone().unwrap();

    let space_a = node_a.create_space("Event Test Space").await?;
    let space_b = node_b.join_space(space_a.create_invite().await?).await?;

    let author_a = node_a.author.unwrap();
    let room_a = space_a.create_call_room(author_a, "Event Test Call").await?;

    // subscribe before A joins so we don't miss the event
    let mut events_a = node_a.events.subscribe();

    let (_call_a, _mesh_a) = node_a.join_call_room(space_a.id(), &room_a).await?;

    let id_a = node_a.endpoint.as_ref().unwrap().id().to_string();
    let id_b = node_b.endpoint.as_ref().unwrap().id().to_string();

    let mut room_b = None;
    for _ in 0..100 {
        space_b.sync_call_rooms(&blobs_b).await?;
        let rooms = space_b.call_rooms.lock().await;
        if let Some(r) = rooms.iter().find(|r| r.id == room_a.id) {
            room_b = Some(r.clone());
            break;
        }
        drop(rooms);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let room_b = room_b.expect("B never discovered the call room");
    for _ in 0..100 {
        if room_b.list_active_members(blobs_b.clone()).await?.contains(&id_a) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    // B dials into A - this is the accept-side path that used to never fire
    // NewCallParticipant because `room_spaces` was never populated
    let (_call_b, _mesh_b) = node_b.join_call_room(space_b.id(), &room_b).await?;

    let saw_event = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            match events_a.recv().await {
                Ok(blabber_core::AppEvent::NewCallParticipant { room_id, endpoint_id, .. })
                    if room_id == room_a.id && endpoint_id == id_b =>
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

    assert!(
        saw_event,
        "A never received a NewCallParticipant event for B dialing in - room_spaces regression"
    );

    Ok(())
}
