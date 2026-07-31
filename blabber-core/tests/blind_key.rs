use anyhow::Result;

mod common;
use common::make_node;

#[tokio::test]
async fn room_with_no_key_reads_nothing_and_cannot_send() -> Result<()> {
    let (node_a, _dir_a) = make_node("BlindKeyAlice").await?;
    let (node_blind, _dir_blind) = make_node("BlindKeyRelay").await?;

    let space_a = node_a.create_space("Blind Key Space").await?;
    let author_a = space_a.author().unwrap();
    let room_a = space_a.create_room(&node_a, author_a, "general").await?;
    room_a.send_message(author_a, "visible only with the key").await?;

    let blobs_a = node_a.blobs.clone().unwrap();

    // give the doc a moment to actually persist the write locally
    let mut history = Vec::new();
    for _ in 0..50 {
        history = room_a.list_messages(blobs_a.clone()).await?;
        if !history.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(!history.is_empty(), "sanity check: the keyed room should see its own message");

    // a second node imports the same docs read-only, with no space key at all -
    // the exact shape a blind relay is in
    let messages_ticket = room_a
        .messages
        .share(
            iroh_docs::api::protocol::ShareMode::Read,
            iroh_docs::api::protocol::AddrInfoOptions::RelayAndAddresses,
        )
        .await?;
    let media_ticket = room_a
        .media
        .share(
            iroh_docs::api::protocol::ShareMode::Read,
            iroh_docs::api::protocol::AddrInfoOptions::RelayAndAddresses,
        )
        .await?;
    let docs_blind = node_blind.docs.as_ref().unwrap();
    let blobs_blind = node_blind.blobs.clone().unwrap();

    let blind_room = blabber_core::room::Room::from_ticket(
        docs_blind,
        room_a.id,
        "irrelevant-without-key".to_string(),
        messages_ticket,
        media_ticket,
        None,
    )
    .await?;

    // give the blind side's doc sync/blob download a moment to actually settle
    let mut blind_history = Vec::new();
    for _ in 0..50 {
        blind_history = blind_room.list_messages(blobs_blind.clone()).await?;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(
        blind_history.is_empty(),
        "a room with no key must never return decoded messages, even if the ciphertext synced"
    );

    let send_result = blind_room.send_message(author_a, "should be refused").await;
    assert!(
        send_result.is_err(),
        "a room with no key must refuse to originate a message"
    );

    Ok(())
}

#[tokio::test]
async fn call_room_with_no_key_reads_nothing_and_cannot_publish() -> Result<()> {
    let (node_a, _dir_a) = make_node("BlindKeyCallAlice").await?;
    let (node_blind, _dir_blind) = make_node("BlindKeyCallRelay").await?;

    let space_a = node_a.create_space("Blind Key Call Space").await?;
    let author_a = space_a.author().unwrap();
    let call_room_a = space_a.create_call_room(&node_a, author_a, "voice").await?;
    call_room_a
        .set_membership(author_a, "alice-endpoint".to_string(), true)
        .await?;

    let blobs_a = node_a.blobs.clone().unwrap();

    let mut active = Vec::new();
    for _ in 0..50 {
        active = call_room_a.list_active_members(blobs_a.clone()).await?;
        if !active.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(!active.is_empty(), "sanity check: the keyed call room should see its own membership");

    let ticket = call_room_a
        .call_log
        .share(
            iroh_docs::api::protocol::ShareMode::Read,
            iroh_docs::api::protocol::AddrInfoOptions::RelayAndAddresses,
        )
        .await?;
    let docs_blind = node_blind.docs.as_ref().unwrap();
    let blobs_blind = node_blind.blobs.clone().unwrap();

    let blind_call_room = blabber_core::call_rooms::CallRoom::from_ticket(
        docs_blind,
        call_room_a.id,
        "irrelevant-without-key".to_string(),
        ticket,
        None,
    )
    .await?;

    let mut blind_active = Vec::new();
    for _ in 0..50 {
        blind_active = blind_call_room.list_active_members(blobs_blind.clone()).await?;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(
        blind_active.is_empty(),
        "a call room with no key must never return decoded membership, even if the ciphertext synced"
    );

    let publish_result = blind_call_room
        .set_membership(author_a, "attacker-endpoint".to_string(), true)
        .await;
    assert!(
        publish_result.is_err(),
        "a call room with no key must refuse to publish presence"
    );

    Ok(())
}
