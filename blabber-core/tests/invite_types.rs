//! Round-trip and cross-type rejection tests for the two invite wire
//! formats: `Invite` (full member, carries the space's decryption key) and
//! `RelayInvite` (blind relay, read-only, no key). These are pure
//! serialization tests - no real node/network setup needed, since both
//! types' fields are public and can be constructed directly.

use anyhow::Result;
use blabber_core::invite::{Invite, RelayInvite};
use uuid::Uuid;

fn sample_invite() -> Invite {
    Invite {
        space_id: Uuid::new_v4(),
        space_name: "Test Space".to_string(),
        info_ticket: "info-ticket-placeholder".to_string(),
        member_ticket: "member-ticket-placeholder".to_string(),
        space_key: [7u8; 32],
    }
}

fn sample_relay_invite() -> RelayInvite {
    RelayInvite {
        space_id: Uuid::new_v4(),
        space_name: "Test Space".to_string(),
        info_ticket: "info-ticket-placeholder".to_string(),
        member_ticket: "member-ticket-placeholder".to_string(),
    }
}

#[test]
fn invite_round_trips_through_serialize_and_deserialize() -> Result<()> {
    let invite = sample_invite();
    let ticket = invite.serialize_invite()?;
    let round_tripped = Invite::deserialize_invite(ticket)?;

    assert_eq!(round_tripped.space_id, invite.space_id);
    assert_eq!(round_tripped.space_name, invite.space_name);
    assert_eq!(round_tripped.info_ticket, invite.info_ticket);
    assert_eq!(round_tripped.member_ticket, invite.member_ticket);
    assert_eq!(round_tripped.space_key, invite.space_key);
    Ok(())
}

#[test]
fn relay_invite_round_trips_through_serialize_and_deserialize() -> Result<()> {
    let invite = sample_relay_invite();
    let ticket = invite.serialize_invite()?;
    let round_tripped = RelayInvite::deserialize_invite(ticket)?;

    assert_eq!(round_tripped.space_id, invite.space_id);
    assert_eq!(round_tripped.space_name, invite.space_name);
    assert_eq!(round_tripped.info_ticket, invite.info_ticket);
    assert_eq!(round_tripped.member_ticket, invite.member_ticket);
    Ok(())
}

#[test]
fn invite_round_trips_through_encrypted_local_storage() -> Result<()> {
    let invite = sample_invite();
    let key = [3u8; 32];
    let encrypted = invite.serialize_invite_encrypted(&key)?;
    let round_tripped = Invite::deserialize_invite_encrypted(&encrypted, &key)?;

    assert_eq!(round_tripped.space_id, invite.space_id);
    assert_eq!(round_tripped.space_key, invite.space_key);
    Ok(())
}

#[test]
fn relay_invite_round_trips_through_encrypted_local_storage() -> Result<()> {
    let invite = sample_relay_invite();
    let key = [3u8; 32];
    let encrypted = invite.serialize_invite_encrypted(&key)?;
    let round_tripped = RelayInvite::deserialize_invite_encrypted(&encrypted, &key)?;

    assert_eq!(round_tripped.space_id, invite.space_id);
    Ok(())
}

/// The core security property from the blind-relay redesign: a relay-invite
/// ticket string must never be accepted by the full-member `Invite`
/// deserializer (and vice versa), so a human invite (carrying the space key)
/// pasted into blabber-root's config fails to parse rather than silently
/// working.
#[test]
fn relay_invite_ticket_is_rejected_by_invite_deserializer() -> Result<()> {
    let relay_ticket = sample_relay_invite().serialize_invite()?;
    assert!(Invite::deserialize_invite(relay_ticket).is_err());
    Ok(())
}

#[test]
fn invite_ticket_is_rejected_by_relay_invite_deserializer() -> Result<()> {
    let member_ticket = sample_invite().serialize_invite()?;
    assert!(RelayInvite::deserialize_invite(member_ticket).is_err());
    Ok(())
}

#[test]
fn relay_invite_encrypted_payload_is_rejected_by_invite_deserializer() -> Result<()> {
    let key = [9u8; 32];
    let encrypted = sample_relay_invite().serialize_invite_encrypted(&key)?;
    assert!(Invite::deserialize_invite_encrypted(&encrypted, &key).is_err());
    Ok(())
}

#[test]
fn invite_encrypted_payload_is_rejected_by_relay_invite_deserializer() -> Result<()> {
    let key = [9u8; 32];
    let encrypted = sample_invite().serialize_invite_encrypted(&key)?;
    assert!(RelayInvite::deserialize_invite_encrypted(&encrypted, &key).is_err());
    Ok(())
}
