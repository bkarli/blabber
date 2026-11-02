//! a real node's on-disk state across restarts, leaving and deletion.

use anyhow::{Context, Result};
use blabber_core::identity::Identity;
use blabber_core::node::Node;
use uuid::Uuid;

#[tokio::test]
async fn persisted_space_is_reloaded_after_restart_and_damaged_invites_are_skipped() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let identity_path = dir.path().join("identity.bin");
    let identity = Identity::new("Alice");
    identity.store("password", &identity_path)?;

    let mut node = Node::new(identity);
    node.run(dir.path()).await?;
    let space = node.create_space("Test Space").await?;
    node.persist_space(&space).await?;
    let space_id = space.id();
    let storage = node.storage()?.clone();
    node.shutdown().await?;

    // one invite that won't decrypt, and one that can't be read at all.
    storage.save_invite(Uuid::new_v4(), b"not a valid invite".to_vec()).await?;
    std::fs::create_dir_all(storage.invite_path(Uuid::new_v4()))?;

    let mut node = Node::new(Identity::load_from_disk(&identity_path, "password")?);
    node.run(dir.path()).await?;
    let loaded = node.load_spaces().await?;
    assert_eq!(loaded.len(), 1, "only the valid space should load");
    assert_eq!(loaded.first().context("no space loaded")?.id(), space_id);

    node.leave_space(space_id).await?;
    assert!(!storage.space_dir(space_id).exists(), "leaving should remove the space's files");

    node.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn remove_all_after_shutdown_deletes_the_identitys_stores_and_spaces() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let mut node = Node::new(Identity::new("Alice"));
    node.run(dir.path()).await?;
    let space = node.create_space("Test Space").await?;
    node.persist_space(&space).await?;

    let storage = node.storage()?.clone();
    assert!(storage.blobs_dir().exists());
    assert!(storage.invite_path(space.id()).exists());

    drop(space);
    node.shutdown().await?;
    storage.remove_all().await?;

    assert!(!storage.blobs_dir().exists());
    assert!(!storage.spaces_dir().exists());
    Ok(())
}
