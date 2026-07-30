//! Reproduces a reported bug: after leaving a space and rejoining it, rooms
//! didn't load and other peers didn't see the rejoined member. Runs two real
//! `Node`s in-process over real local Iroh endpoints.

use anyhow::Result;

mod common;
use common::make_node;

#[tokio::test]
async fn rejoin_after_leaving_resyncs_rooms_and_membership() -> Result<()> {
    let (node_a, _dir_a) = make_node("RejoinAlice").await?;
    let (node_b, dir_b) = make_node("RejoinBob").await?;

    let space_a = node_a.create_space("Rejoin Test Space").await?;
    let author_a = space_a.author().unwrap();
    let _room_a = space_a.create_room(&node_a, author_a, "general").await?;

    let invite1 = space_a.create_invite().await?;
    let space_b = node_b.join_space(invite1).await?;
    let blobs_b = node_b.blobs.clone().unwrap();

    // B should see the pre-existing room right after the initial join
    let mut saw_room_initially = false;
    for _ in 0..50 {
        space_b.sync_rooms(&node_b, &blobs_b).await?;
        if !space_b.rooms.lock().await.is_empty() {
            saw_room_initially = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(saw_room_initially, "B should see the existing room on initial join");

    // A should see B as a member
    let blobs_a = node_a.blobs.clone().unwrap();
    let id_b = node_b.endpoint.as_ref().unwrap().id().to_string();
    let mut members_a = vec![];
    for _ in 0..50 {
        members_a = space_a.list_members(&blobs_a).await?;
        if members_a.iter().any(|m| m.endpoint_id == id_b) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(
        members_a.iter().any(|m| m.endpoint_id == id_b),
        "A should see B as a member after the initial join"
    );

    // B leaves
    let spaces_root = dir_b.path().join("spaces");
    let space_b_id = space_b.id();
    node_b.leave_space(space_b_id, spaces_root.clone()).await?;

    // A should observe B leaving
    let mut members_a_after_leave = vec![];
    for _ in 0..50 {
        members_a_after_leave = space_a.list_members(&blobs_a).await?;
        if !members_a_after_leave.iter().any(|m| m.endpoint_id == id_b) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(
        !members_a_after_leave.iter().any(|m| m.endpoint_id == id_b),
        "A should see B leave"
    );

    // B rejoins using a fresh invite from A
    let invite2 = space_a.create_invite().await?;
    let space_b2 = node_b.join_space(invite2).await?;

    // does B see the room after rejoining?
    let mut saw_room_after_rejoin = false;
    for _ in 0..50 {
        space_b2.sync_rooms(&node_b, &blobs_b).await?;
        if !space_b2.rooms.lock().await.is_empty() {
            saw_room_after_rejoin = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(
        saw_room_after_rejoin,
        "B should see the existing room after rejoining - this is the reported bug"
    );

    // does A see B as a member again?
    let mut members_a_after_rejoin = vec![];
    for _ in 0..50 {
        members_a_after_rejoin = space_a.list_members(&blobs_a).await?;
        if members_a_after_rejoin.iter().any(|m| m.endpoint_id == id_b) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(
        members_a_after_rejoin.iter().any(|m| m.endpoint_id == id_b),
        "A should see B as a member again after B rejoins - this is the reported bug"
    );

    Ok(())
}

/// Same scenario, but B rejoins by reusing the exact same invite string
/// they originally used (e.g. pasted from an old chat message/clipboard),
/// rather than asking A for a fresh one.
#[tokio::test]
async fn rejoin_with_same_invite_string_resyncs_rooms_and_membership() -> Result<()> {
    use blabber_core::invite::Invite;

    let (node_a, _dir_a) = make_node("SameInviteAlice").await?;
    let (node_b, dir_b) = make_node("SameInviteBob").await?;

    let space_a = node_a.create_space("Same Invite Test Space").await?;
    let author_a = space_a.author().unwrap();
    let _room_a = space_a.create_room(&node_a, author_a, "general").await?;

    let invite = space_a.create_invite().await?;
    let invite_str = invite.serialize_invite()?;
    let space_b = node_b.join_space(invite).await?;
    let blobs_b = node_b.blobs.clone().unwrap();

    let mut saw_room_initially = false;
    for _ in 0..50 {
        space_b.sync_rooms(&node_b, &blobs_b).await?;
        if !space_b.rooms.lock().await.is_empty() {
            saw_room_initially = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(saw_room_initially, "B should see the existing room on initial join");

    let spaces_root = dir_b.path().join("spaces");
    let space_b_id = space_b.id();
    node_b.leave_space(space_b_id, spaces_root.clone()).await?;

    let blobs_a = node_a.blobs.clone().unwrap();
    let id_b = node_b.endpoint.as_ref().unwrap().id().to_string();
    for _ in 0..50 {
        let members = space_a.list_members(&blobs_a).await?;
        if !members.iter().any(|m| m.endpoint_id == id_b) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    // rejoin with the SAME invite string, not a fresh one
    let reused_invite = Invite::deserialize_invite(invite_str)?;
    let space_b2 = node_b.join_space(reused_invite).await?;

    let mut saw_room_after_rejoin = false;
    for _ in 0..50 {
        space_b2.sync_rooms(&node_b, &blobs_b).await?;
        if !space_b2.rooms.lock().await.is_empty() {
            saw_room_after_rejoin = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(
        saw_room_after_rejoin,
        "B should see the existing room after rejoining with the same invite string"
    );

    let mut members_a_after_rejoin = vec![];
    for _ in 0..50 {
        members_a_after_rejoin = space_a.list_members(&blobs_a).await?;
        if members_a_after_rejoin.iter().any(|m| m.endpoint_id == id_b) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(
        members_a_after_rejoin.iter().any(|m| m.endpoint_id == id_b),
        "A should see B as a member again after B rejoins with the same invite string"
    );

    Ok(())
}
