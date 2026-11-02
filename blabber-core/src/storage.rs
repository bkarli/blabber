use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use uuid::Uuid;

const BLOBS_DIR: &str = "blobs";
const SPACES_DIR: &str = "spaces";
const META_DIR: &str = "meta";
const INVITE_FILE: &str = "invite.txt";

/// invite read from disk, still encrypted.
#[derive(Debug)]
pub struct StoredInvite {
    pub space_id: Uuid,
    pub encrypted: Vec<u8>,
}

/// paths and stored invites for one identity.
#[derive(Debug, Clone)]
pub struct NodeStorage {
    blobs_dir: PathBuf,
    spaces_dir: PathBuf,
}

/// turns user supplied display name into file system safe path
pub fn sanitize_path_component(name: &str) -> Option<String> {
    let sanitized: String = name
        .chars()
        .filter(|character| character.is_alphanumeric() || *character == '-' || *character == '_')
        .collect();
    if sanitized.is_empty() {
        None
    } else {
        Some(sanitized)
    }
}

impl NodeStorage {
    pub fn new(data_dir: impl AsRef<Path>, display_name: &str) -> Result<Self> {
        let name = sanitize_path_component(display_name)
            .context("Unable to sanitize path component")?;
        let data_dir = data_dir.as_ref();
        Ok(Self {
            blobs_dir: data_dir.join(BLOBS_DIR).join(&name),
            spaces_dir: data_dir.join(SPACES_DIR).join(&name),
        })
    }

    pub fn blobs_dir(&self) -> &Path {
        &self.blobs_dir
    }

    pub fn spaces_dir(&self) -> &Path {
        &self.spaces_dir
    }

    pub fn space_dir(&self, space_id: Uuid) -> PathBuf {
        self.spaces_dir.join(space_id.to_string())
    }

    pub fn invite_path(&self, space_id: Uuid) -> PathBuf {
        self.space_dir(space_id).join(META_DIR).join(INVITE_FILE)
    }

    pub async fn save_invite(&self, space_id: Uuid, encrypted: Vec<u8>) -> Result<()> {
        let path = self.invite_path(space_id);
        tokio::task::spawn_blocking(move || {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("creating {}", parent.display()))?;
            }
            atomic_write(&path, &encrypted)
        })
        .await
        .context("invite write task failed")?
    }

    /// reads every stored invite.
    pub async fn load_invites(&self) -> Result<Vec<StoredInvite>> {
        let mut entries = match tokio::fs::read_dir(&self.spaces_dir).await {
            Ok(entries) => entries,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => {
                return Err(e).with_context(|| format!("reading {}", self.spaces_dir.display()));
            }
        };

        let mut invites = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .with_context(|| format!("scanning {}", self.spaces_dir.display()))?
        {
            let file_name = entry.file_name();
            let Some(space_id) = file_name.to_str().and_then(|name| Uuid::parse_str(name).ok())
            else {
                continue;
            };

            let path = self.invite_path(space_id);
            match tokio::fs::read(&path).await {
                Ok(encrypted) => invites.push(StoredInvite { space_id, encrypted }),
                // not a space directory, or one without an invite: nothing to load.
                Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {}
                Err(e) => eprintln!("skipping unreadable invite {}: {e}", path.display()),
            }
        }
        Ok(invites)
    }

    pub async fn remove_space(&self, space_id: Uuid) -> Result<()> {
        remove_dir_if_exists(&self.space_dir(space_id)).await
    }

    pub async fn remove_all(&self) -> Result<()> {
        remove_dir_if_exists(&self.blobs_dir).await?;
        remove_dir_if_exists(&self.spaces_dir).await
    }
}

async fn remove_dir_if_exists(path: &Path) -> Result<()> {
    match tokio::fs::remove_dir_all(path).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| format!("removing {}", path.display())),
    }
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let file_name = path
        .file_name()
        .with_context(|| format!("{} no file name", path.display()))?;
    let mut temp_name = OsString::from(".");
    temp_name.push(file_name);
    temp_name.push(format!(".{}.tmp", Uuid::new_v4()));
    let temp_path = path.with_file_name(temp_name);

    let written = write_and_sync(&temp_path, bytes).and_then(|()| {
        std::fs::rename(&temp_path, path)
            .with_context(|| format!("replacing {}", path.display()))
    });
    if let Err(e) = written {
        let _ = std::fs::remove_file(&temp_path);
        return Err(e);
    }

    sync_parent_dir(path)
}

fn write_and_sync(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let mut file = options
        .open(path)
        .with_context(|| format!("creating {}", path.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("writing {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("syncing {}", path.display()))
}

/// windows can't open a directory this way, so there the rename is left to the filesystem.
#[cfg(unix)]
fn sync_parent_dir(path: &Path) -> Result<()> {
    // the parent of a bare file name is an empty path, which can't be opened.
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    std::fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .with_context(|| format!("syncing {}", parent.display()))
}

#[cfg(not(unix))]
fn sync_parent_dir(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// everything left in a directory, for spotting leftover temp files.
    fn file_names(directory: &Path) -> Result<Vec<OsString>> {
        std::fs::read_dir(directory)?
            .map(|entry| Ok(entry?.file_name()))
            .collect()
    }

    #[test]
    fn new_rejects_a_name_with_no_path_safe_characters() {
        assert!(NodeStorage::new("data", "../!!").is_err());
    }

    #[test]
    fn new_strips_path_separators_from_the_name() -> Result<()> {
        let storage = NodeStorage::new("data", "../../evil")?;
        assert_eq!(storage.blobs_dir(), Path::new("data/blobs/evil"));
        assert_eq!(storage.spaces_dir(), Path::new("data/spaces/evil"));
        Ok(())
    }

    #[test]
    fn invite_path_matches_the_existing_on_disk_layout() -> Result<()> {
        let storage = NodeStorage::new("data", "alice")?;
        let space_id = Uuid::new_v4();
        let expected = format!("data/spaces/alice/{space_id}/meta/invite.txt");
        assert_eq!(storage.invite_path(space_id), PathBuf::from(expected));
        Ok(())
    }

    #[test]
    fn different_identities_get_separate_directories() -> Result<()> {
        let alice = NodeStorage::new("data", "alice")?;
        let bob = NodeStorage::new("data", "bob")?;
        assert_ne!(alice.blobs_dir(), bob.blobs_dir());
        assert_ne!(alice.spaces_dir(), bob.spaces_dir());
        Ok(())
    }

    #[tokio::test]
    async fn saved_invite_is_returned_by_load_invites() -> Result<()> {
        let dir = tempdir()?;
        let storage = NodeStorage::new(dir.path(), "alice")?;
        let space_id = Uuid::new_v4();

        storage.save_invite(space_id, b"first".to_vec()).await?;
        storage.save_invite(space_id, b"second".to_vec()).await?;

        let invites = storage.load_invites().await?;
        assert_eq!(invites.len(), 1);
        assert_eq!(invites[0].space_id, space_id);
        assert_eq!(invites[0].encrypted, b"second");
        Ok(())
    }

    #[tokio::test]
    async fn load_invites_returns_nothing_when_spaces_dir_is_missing() -> Result<()> {
        let dir = tempdir()?;
        let storage = NodeStorage::new(dir.path(), "alice")?;
        assert!(storage.load_invites().await?.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn load_invites_skips_entries_that_are_not_spaces() -> Result<()> {
        let dir = tempdir()?;
        let storage = NodeStorage::new(dir.path(), "alice")?;
        let space_id = Uuid::new_v4();
        storage.save_invite(space_id, b"valid".to_vec()).await?;

        std::fs::create_dir(storage.spaces_dir().join("not-a-uuid"))?;
        std::fs::create_dir(storage.space_dir(Uuid::new_v4()))?;
        std::fs::write(storage.space_dir(Uuid::new_v4()), b"a file, not a directory")?;

        let invites = storage.load_invites().await?;
        assert_eq!(invites.len(), 1);
        assert_eq!(invites[0].space_id, space_id);
        Ok(())
    }

    #[tokio::test]
    async fn load_invites_keeps_other_spaces_when_one_invite_is_unreadable() -> Result<()> {
        let dir = tempdir()?;
        let storage = NodeStorage::new(dir.path(), "alice")?;
        let space_id = Uuid::new_v4();
        storage.save_invite(space_id, b"valid".to_vec()).await?;

        // a directory at the invite's path makes reading it fail on every platform.
        std::fs::create_dir_all(storage.invite_path(Uuid::new_v4()))?;

        let invites = storage.load_invites().await?;
        assert_eq!(invites.len(), 1);
        assert_eq!(invites[0].space_id, space_id);
        Ok(())
    }

    #[tokio::test]
    async fn remove_space_only_removes_that_space_and_succeeds_when_already_gone() -> Result<()> {
        let dir = tempdir()?;
        let storage = NodeStorage::new(dir.path(), "alice")?;
        let removed = Uuid::new_v4();
        let kept = Uuid::new_v4();
        storage.save_invite(removed, b"removed".to_vec()).await?;
        storage.save_invite(kept, b"kept".to_vec()).await?;

        storage.remove_space(removed).await?;
        storage.remove_space(removed).await?;

        let invites = storage.load_invites().await?;
        assert_eq!(invites.len(), 1);
        assert_eq!(invites[0].space_id, kept);
        Ok(())
    }

    #[tokio::test]
    async fn remove_all_keeps_other_identities_data() -> Result<()> {
        let dir = tempdir()?;
        let alice = NodeStorage::new(dir.path(), "alice")?;
        let bob = NodeStorage::new(dir.path(), "bob")?;
        for storage in [&alice, &bob] {
            std::fs::create_dir_all(storage.blobs_dir())?;
            storage.save_invite(Uuid::new_v4(), b"invite".to_vec()).await?;
        }

        alice.remove_all().await?;
        alice.remove_all().await?;

        assert!(!alice.blobs_dir().exists());
        assert!(!alice.spaces_dir().exists());
        assert!(bob.blobs_dir().exists());
        assert_eq!(bob.load_invites().await?.len(), 1);
        Ok(())
    }

    #[test]
    fn atomic_write_replaces_existing_content_and_leaves_no_temp_file() -> Result<()> {
        let dir = tempdir()?;
        let path = dir.path().join("file.bin");

        atomic_write(&path, b"old")?;
        atomic_write(&path, b"new")?;

        assert_eq!(std::fs::read(&path)?, b"new");
        assert_eq!(file_names(dir.path())?, [OsString::from("file.bin")]);
        Ok(())
    }

    #[test]
    fn atomic_write_keeps_the_target_and_cleans_up_when_the_rename_fails() -> Result<()> {
        let dir = tempdir()?;
        // renaming a file over a non-empty directory fails.
        let path = dir.path().join("target");
        std::fs::create_dir(&path)?;
        std::fs::write(path.join("child"), b"kept")?;

        assert!(atomic_write(&path, b"new").is_err());

        assert_eq!(std::fs::read(path.join("child"))?, b"kept");
        assert_eq!(file_names(dir.path())?, [OsString::from("target")]);
        Ok(())
    }

    #[test]
    fn atomic_write_fails_when_the_parent_directory_is_missing() -> Result<()> {
        let dir = tempdir()?;
        assert!(atomic_write(&dir.path().join("missing/file.bin"), b"data").is_err());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_sets_owner_only_permissions_on_unix() -> Result<()> {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir()?;
        let path = dir.path().join("file.bin");
        std::fs::write(&path, b"old")?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))?;

        atomic_write(&path, b"new")?;

        let mode = std::fs::metadata(&path)?.permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        Ok(())
    }
}
