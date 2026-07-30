use anyhow::Result;
use n0_future::StreamExt;

mod common;
use common::make_node;

#[tokio::test]
async fn relay_never_decrypts_but_still_propagates_ciphertext() -> Result<()> {
    let (node_a, _dir_a) = make_node("RelayFlowAlice").await?;
    let (node_relay, _dir_relay) = make_node("RelayFlowRelay").await?;

    let space_a = node_a.create_space("Relay Flow Space").await?;
    let author_a = space_a.author().unwrap();
    let room_a = space_a.create_room(&node_a, author_a, "general").await?;
    let call_room_a = space_a.create_call_room(author_a, "voice").await?;

    room_a.send_message(author_a, "visible only to real members").await?;
    call_room_a
        .set_membership(author_a, "alice-endpoint".to_string(), true)
        .await?;

    // the relay joins via a RelayInvite: read-only content, no space key
    let relay_invite = space_a.create_relay_invite().await?;
    let space_relay = node_relay.join_space_relay(relay_invite).await?;

    assert!(
        space_relay.author().is_some(),
        "a relay-joined space now has a locally-derived author, used only to sign its own cleartext presence entry"
    );
    assert!(space_relay.key().is_none(), "a relay-joined space must have no key");

    // it discovers the pre-existing room/call room the same way a
    // later joining member does
    let blobs_relay = node_relay.blobs.clone().unwrap();
    space_relay.sync_rooms(&node_relay, &blobs_relay).await?;
    space_relay.sync_call_rooms(&blobs_relay).await?;

    // the relay never learns real (encrypted) member records - the only
    // entries it can ever see in its own view of the members doc are
    // cleartext relay/ presence entries, including its own
    let relay_members = space_relay.list_members(&blobs_relay).await?;
    assert!(
        relay_members.iter().all(|m| m.is_relay),
        "a blind relay must never decode a real (encrypted) member record"
    );

    // and a real member should, in turn, see the relay's own presence -
    // clearly marked as a relay, not a human
    let blobs_a = node_a.blobs.clone().unwrap();
    let mut alice_sees_relay = false;
    for _ in 0..50 {
        let members = space_a.list_members(&blobs_a).await?;
        if members.iter().any(|m| m.is_relay) {
            alice_sees_relay = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(alice_sees_relay, "a real member should see the relay's presence in the member list");

    let room_relay = {
        let mut found = None;
        for _ in 0..50 {
            let rooms = space_relay.rooms.lock().await;
            if let Some(r) = rooms.iter().find(|r| r.id == room_a.id) {
                found = Some(r.clone());
                break;
            }
            drop(rooms);
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        found.expect("relay never discovered the pre-existing room")
    };
    let relay_messages = room_relay.list_messages(blobs_relay.clone()).await?;
    assert!(relay_messages.is_empty(), "a blind relay must never decrypt message content");

    let call_room_relay = {
        let mut found = None;
        for _ in 0..50 {
            let call_rooms = space_relay.call_rooms.lock().await;
            if let Some(r) = call_rooms.iter().find(|r| r.id == call_room_a.id) {
                found = Some(r.clone());
                break;
            }
            drop(call_rooms);
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        found.expect("relay never discovered the pre-existing call room")
    };
    let relay_active = call_room_relay.list_active_members(blobs_relay.clone()).await?;
    assert!(relay_active.is_empty(), "a blind relay must never decode call presence");

    // but it does replicate the ciphertext locally
    let mut saw_ciphertext = false;
    for _ in 0..50 {
        let entries = room_relay
            .messages
            .get_many(iroh_docs::store::Query::single_latest_per_key().key_prefix("msg/"))
            .await?;
        let mut entries = std::pin::pin!(entries);
        while let Some(entry) = entries.next().await {
            let entry = entry?;
            if blobs_relay.blobs().get_bytes(entry.content_hash()).await.is_ok() {
                saw_ciphertext = true;
            }
        }
        if saw_ciphertext {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(
        saw_ciphertext,
        "relay should hold the message's ciphertext blob locally, even though it can't decrypt it"
    );

    let send_result = room_relay.send_message(author_a, "should be refused").await;
    assert!(send_result.is_err(), "a blind relay's Room must refuse to send");

    let publish_result = call_room_relay
        .set_membership(author_a, "relay-endpoint".to_string(), true)
        .await;
    assert!(
        publish_result.is_err(),
        "a blind relay's CallRoom must refuse to publish presence"
    );

    Ok(())
}

#[tokio::test]
async fn relay_discovers_room_created_after_it_joined() -> Result<()> {
    let (node_a, _dir_a) = make_node("RelayLateRoomAlice").await?;
    let (node_relay, _dir_relay) = make_node("RelayLateRoomRelay").await?;

    let space_a = node_a.create_space("Relay Late Room Space").await?;
    let author_a = space_a.author().unwrap();

    let relay_invite = space_a.create_relay_invite().await?;
    let space_relay = node_relay.join_space_relay(relay_invite).await?;
    let blobs_relay = node_relay.blobs.clone().unwrap();
    space_relay.sync_rooms(&node_relay, &blobs_relay).await?;
    assert!(space_relay.rooms.lock().await.is_empty());

    // NOW A creates a room, after the relay already joined
    let room_a = space_a.create_room(&node_a, author_a, "late-room").await?;

    let mut discovered = false;
    for _ in 0..50 {
        if space_relay.rooms.lock().await.iter().any(|r| r.id == room_a.id) {
            discovered = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(discovered, "relay should discover a room created after it joined, via the live info-doc watcher");

    Ok(())
}
