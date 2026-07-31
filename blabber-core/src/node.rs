use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use iroh::{protocol::Router, Endpoint, SecretKey, endpoint::presets};
use iroh_blobs::store::fs::FsStore;
use iroh_blobs::{ALPN as BLOBS_ALPN, BlobsProtocol};
use iroh_docs::api::Doc;
use iroh_docs::engine::LiveEvent;
use iroh_docs::{protocol::Docs, AuthorId, ALPN as DOCS_ALPN};
use iroh_gossip::{Gossip, ALPN as GOSSIP_ALPN};
use n0_future::StreamExt;
use tokio::fs;
use tokio::sync::{Mutex, broadcast};
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::events::AppEvent;
use crate::invite::{Invite, RelayInvite};
use crate::space::Space;
use crate::Identity;

pub struct Node {
    identity: Identity,
    pub endpoint: Option<Endpoint>,
    pub gossip: Option<Gossip>,
    pub router: Option<Router>,
    pub blobs: Option<FsStore>,
    pub docs: Option<Docs>,

    // Node can produce App Events and GUI can subscribe to these events
    pub events: broadcast::Sender<AppEvent>,
    pub spaces: Arc<Mutex<Vec<Space>>>,
    /// mesh call rooms this node is currently participating in, keyed by room id
    pub active_call_rooms: crate::channel::ActiveCallRooms,
    /// maps a call room id back to the space it belongs to, so an inbound
    /// mesh connection can be attributed to the right space
    pub room_spaces: crate::channel::RoomSpaceMap,
    /// shared audio engine: owns device selection and mixes call audio and
    /// sound effects into one output stream
    pub sound: Arc<crate::sound::SoundHandler>,
}

impl Node {
    pub fn new(identity: Identity) -> Self {
        let (events, _) = broadcast::channel(256);
        Self {
            identity,
            endpoint: None,
            gossip: None,
            router: None,
            blobs: None,
            docs: None,
            events,
            spaces: Arc::new(Mutex::new(Vec::new())),
            active_call_rooms: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            room_spaces: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            sound: Arc::new(crate::sound::SoundHandler::new()),
        }
    }

    /// Scopes a shared root path (blob store, spaces directory, ...) under
    /// this identity's own subdirectory, so multiple identities on the same
    /// machine never collide.
    pub fn identity_scoped_path(&self, path: &PathBuf) -> Result<PathBuf> {
        let identity_dir = crate::identity::sanitize_path_component(&self.identity.display_name)
            .ok_or_else(|| anyhow!("identity display name has no path-safe characters"))?;
        Ok(path.join(identity_dir))
    }

    /// produces key used to encrypt persisted invite.txt files
    pub fn local_storage_key(&self) -> [u8; 32] {
        blake3::derive_key("blabber-app invite local storage v1", self.identity.secret.as_bytes())
    }

    /// Helpers for option typed fields, has to account for the possibility to option isnt ready yet.
    fn require_endpoint(&self) -> Result<&Endpoint> {
        self.endpoint.as_ref().context("endpoint not created yet")
    }

    fn require_gossip(&self) -> Result<&Gossip> {
        self.gossip.as_ref().context("gossip not created yet")
    }

    fn require_blobs(&self) -> Result<&FsStore> {
        self.blobs.as_ref().context("blobs not created yet")
    }

    fn require_docs(&self) -> Result<&Docs> {
        self.docs.as_ref().context("docs engine not created yet")
    }

    /// turns secret into an addressable iroh node. Store iroh node in option field.
    pub async fn create_endpoint(&mut self) -> Result<()> {
        let secret_key = SecretKey::from_bytes(self.identity.secret.as_bytes());
        let ep = Endpoint::builder(presets::N0)
            .secret_key(secret_key)
            .alpns(vec![])
            .bind()
            .await?;

        self.endpoint = Some(ep);
        Ok(())
    }

    /// initializes the blob sotre (is on disk).
    pub async fn create_blobs(&mut self, path: &PathBuf) -> Result<()> {
        let blobs = FsStore::load(self.identity_scoped_path(path)?).await?;
        self.blobs = Some(blobs);
        Ok(())
    }

    /// starts pub sub iroh gossip protocol. Makes iroh-docs actually live sync.
    pub async fn create_gossip(&mut self) -> Result<()> {
        let endpoint = self.require_endpoint()?.clone();
        let gossip = Gossip::builder().spawn(endpoint);
        self.gossip = Some(gossip);
        Ok(())
    }

    pub async fn create_docs_engine(&mut self, path: &PathBuf) -> Result<()> {
        let endpoint = self.require_endpoint()?.clone();
        let gossip = self.require_gossip()?.clone();
        let blobs = self.require_blobs()?.clone();

        let docs = Docs::persistent(self.identity_scoped_path(path)?)
            .spawn(endpoint, blobs.into(), gossip)
            .await?;

        self.docs = Some(docs);
        Ok(())
    }

    pub async fn create_router(&mut self) -> Result<()> {
        let endpoint = self.require_endpoint()?.clone();
        let gossip = self.require_gossip()?.clone();
        let blobs = self.require_blobs()?.clone();
        let docs = self.require_docs()?.clone();

        let call_room_protocol = crate::channel::CallRoomProtocol::new(
            self.active_call_rooms.clone(),
            self.room_spaces.clone(),
            self.events.clone(),
        );

        let router = Router::builder(endpoint)
            .accept(GOSSIP_ALPN, gossip)
            .accept(BLOBS_ALPN, BlobsProtocol::new(&blobs, None))
            .accept(DOCS_ALPN, docs)
            .accept(crate::channel::CALL_ROOM_ALPN, call_room_protocol)
            .spawn();

        self.router = Some(router);
        Ok(())
    }

    /// Derives this identity's signing author for a specific space from the
    /// identity secret and the space id, then registers it with the docs
    /// engine so it can sign entries.
    pub async fn space_author(&self, space_id: Uuid) -> Result<AuthorId> {
        let docs = self.require_docs()?;
        let seed = blake3::derive_key(
            "blabber space author v1",
            &[self.identity.secret.as_bytes().as_slice(), space_id.as_bytes()].concat(),
        );
        let author = iroh_docs::Author::from_bytes(&seed);
        let author_id = author.id();
        docs.author_import(author).await?;
        Ok(author_id)
    }

    /// Starts a space's members/info live-sync watchers - shared by every
    /// path that ends up with a freshly constructed `Space`.
    async fn watch_space(&self, space: &Space, blobs: FsStore) -> Result<()> {
        let label = format!("{}/members", space.name());
        space.watch_members(self, blobs.clone(), label).await?;
        let label = format!("{}/info", space.name());
        space.watch_info(self, blobs, label).await?;
        Ok(())
    }

    /// start of the create new space flow. Generates space id and per space author.
    pub async fn create_space(&self, name: impl Into<String>) -> Result<Space> {
        let docs = self.require_docs()?;
        let endpoint_id = self.require_endpoint()?.id().to_string();

        let space_id = Uuid::new_v4();
        let author = self.space_author(space_id).await?;

        let space = Space::new(docs, space_id, author, endpoint_id, self.identity.display_name.clone(), name).await?;
        self.watch_space(&space, self.require_blobs()?.clone()).await?;

        self.spaces.lock().await.push(space.clone());
        Ok(space)
    }

    /// same as create space but for loading one from an invite.
    pub async fn join_space(&self, invite: Invite) -> Result<Space> {
        let docs = self.require_docs()?;
        let endpoint_id = self.require_endpoint()?.id().to_string();
        let author = self.space_author(invite.space_id).await?;

        let space = Space::from_invite(docs, invite, author, endpoint_id, self.identity.display_name.clone()).await?;
        self.watch_space(&space, self.require_blobs()?.clone()).await?;

        self.spaces.lock().await.push(space.clone());
        Ok(space)
    }

    /// Join a space as a blind relay: no decryption key, but it does
    /// publish its own cleartext presence in the members doc so it shows
    /// up - clearly marked as a relay, never as a human - in the member list.
    pub async fn join_space_relay(&self, invite: RelayInvite) -> Result<Space> {
        let docs = self.require_docs()?;
        let endpoint_id = self.require_endpoint()?.id().to_string();
        let author = self.space_author(invite.space_id).await?;

        let space = Space::from_relay_invite(docs, invite, author, endpoint_id, self.identity.display_name.clone()).await?;
        self.watch_space(&space, self.require_blobs()?.clone()).await?;

        self.spaces.lock().await.push(space.clone());
        Ok(space)
    }

    /// Leave a space. remove identity from the space member
    /// list, stop syncing it, drop the local
    /// document replicas, and delete the invite/meta directory so
    /// `load_spaces` won't load it on next launch.
    pub async fn leave_space(&self, space_id: Uuid, spaces_root: PathBuf) -> Result<()> {
        let space = {
            let mut spaces = self.spaces.lock().await;
            let index = spaces
                .iter()
                .position(|space| space.id() == space_id)
                .context("space not found")?;
            spaces.remove(index)
        };

        let author = space
            .author()
            .context("this space has no writable author (blind relay?)")?;
        space.leave(author).await?;

        let user_root = self.identity_scoped_path(&spaces_root)?;
        let space_dir = user_root.join(space_id.to_string());
        if space_dir.is_dir() {
            fs::remove_dir_all(&space_dir).await?;
        }
        Ok(())
    }

    /// Reloads every space this identity previously joined, from
    /// `root_path/<identity>/<space-uuid>/meta/invite.txt`.
    pub async fn load_spaces(&mut self, root_path: PathBuf) -> Result<Vec<Space>> {
        // load spaces-root and create all necessary option fields.
        let root_path = self.identity_scoped_path(&root_path)?;
        tokio::fs::create_dir_all(&root_path).await?;
        let docs = self.require_docs()?;
        let endpoint_id = self.require_endpoint()?.id().to_string();
        let display_name = self.identity.display_name.clone();
        let blobs = self.require_blobs()?;

        // asynchronous directory scan checking for validity
        let mut spaces = Vec::new();
        let mut entries = fs::read_dir(root_path).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            if !path.is_dir() {
                continue;
            }

            let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if Uuid::parse_str(dir_name).is_err() {
                continue;
            }
            let invite_path = path.join("meta").join("invite.txt");
            if !invite_path.is_file() {
                continue;
            }

            // read encrypted invite file and decode it as a member invite.
            let encrypted = fs::read(&invite_path).await?;
            let storage_key = self.local_storage_key();
            let space = if let Ok(invite) = Invite::deserialize_invite_encrypted(&encrypted, &storage_key) {
                let author = match self.space_author(invite.space_id).await {
                    Ok(author) => author,
                    Err(e) => {
                        eprintln!("skipping space {dir_name}: failed to derive author: {e:#}");
                        continue;
                    }
                };
                Space::from_invite(
                    docs,
                    invite,
                    author,
                    endpoint_id.clone(),
                    display_name.clone(),
                ).await?
            } else { // if not a member invite check for relay invite.
                match RelayInvite::deserialize_invite_encrypted(&encrypted, &storage_key) {
                    Ok(invite) => {
                        let author = match self.space_author(invite.space_id).await {
                            Ok(author) => author,
                            Err(e) => {
                                eprintln!("skipping relay space {dir_name}: failed to derive author: {e:#}");
                                continue;
                            }
                        };
                        Space::from_relay_invite(docs, invite, author, endpoint_id.clone(), display_name.clone()).await?
                    }
                    Err(e) => {
                        eprintln!("skipping unreadable space {dir_name}: {e}");
                        continue;
                    }
                }
            };

            // if space is reconstructed, sync all the rooms.
            space
                .sync_rooms(self, blobs)
                .await
                .map_err(|error| {
                    anyhow::anyhow!("failed to sync rooms for space {dir_name}: {error}")
                })?;

            space
                .sync_call_rooms(self, blobs)
                .await
                .map_err(|error| {
                    anyhow::anyhow!("failed to sync call rooms for space {dir_name}: {error}")
                })?;

            // load the members of a space.
            let label = format!("{}/members", space.name());
            space.watch_members(self, blobs.clone(), label).await
                .map_err(|error| anyhow::anyhow!("failed to watch members for space {dir_name}: {error}"))?;

            let label = format!("{}/info", space.name());
            space.watch_info(self, blobs.clone(), label).await
                .map_err(|error| anyhow::anyhow!("failed to watch info for space {dir_name}: {error}"))?;

            spaces.push(space);
        }

        // clone everything that has been found for different ownership. (one for main in blabber-root, and one for space itself)
        self.spaces.lock().await.extend(spaces.clone());
        Ok(spaces)
    }

    /// Thin wrapper around the free `watch_doc` so callers already holding
    /// a `&Node` don't need a separate import.
    pub async fn watch_doc<F, Fut>(&self, doc: Doc, label: impl Into<String>, on_event: F) -> Result<JoinHandle<()>>
    where
        F: Fn(LiveEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        watch_doc(doc, label, on_event).await
    }

    pub async fn shutdown(self) -> Result<()> {
        if let Some(router) = self.router {
            router
                .shutdown()
                .await
                .context("failed to shut down router")?;
        }

        Ok(())
    }

    /// Boots every subsystem in order: endpoint, gossip, blob store, docs engine, router.
    pub async fn run(&mut self, blobs_path: PathBuf) -> Result<()> {
        self.create_endpoint().await?;
        // tests need a connected endpoint before proceeding; production doesn't wait
        #[cfg(test)]
        {
            self.wait_online().await?;
        }
        self.create_gossip().await?;
        self.create_blobs(&blobs_path).await?;
        self.create_docs_engine(&blobs_path).await?;
        self.create_router().await?;
        Ok(())
    }

    
    /// test function to keep the node online
    pub async fn wait_online(&self) -> Result<()> {
        use tokio::time::{timeout, Duration};

        let endpoint = self.require_endpoint()?;

        timeout(Duration::from_secs(5), endpoint.online())
            .await
            .context("timed out waiting for endpoint to come online")?;

        Ok(())
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<AppEvent> {
        self.events.subscribe()
    }

    /// Dial every known peer in a call room and start the local mesh audio pipeline.
    pub async fn join_mesh(
        &self,
        room_id: Uuid,
        my_id: String,
        peers: Vec<(String, iroh::EndpointAddr)>,
    ) -> Result<(crate::channel::MeshActiveCall, crate::channel::MeshVoiceChannel)> {
        let endpoint = self.require_endpoint()?.clone();
        let handle = tokio::runtime::Handle::current();
        let mesh_channel = crate::channel::MeshVoiceChannel::new(
            handle.clone(),
            room_id,
            self.room_spaces.clone(),
            self.events.clone(),
        );
        self.active_call_rooms
            .lock()
            .unwrap()
            .insert(room_id, mesh_channel.clone());
        for (peer_id_str, peer_addr) in peers {
            if peer_id_str == my_id {
                continue;
            }
            let connection = match endpoint.connect(peer_addr, crate::channel::CALL_ROOM_ALPN).await {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("failed to connect to {peer_id_str} for call room {room_id}: {e:#}");
                    continue;
                }
            };
            let Ok((mut send, mut recv)) = connection.open_bi().await else {
                continue;
            };
            if send.write_all(room_id.as_bytes()).await.is_err() {
                continue;
            }
            let _ = send.finish();
            let mut ack = [0u8; 1];
            if recv.read_exact(&mut ack).await.is_ok() && ack[0] == 1 {
                mesh_channel.add_peer(peer_id_str, connection);
            }
        }
        let channel_for_inspection = mesh_channel.clone();
        let call = crate::channel::MeshActiveCall::start(mesh_channel, self.sound.clone());
        Ok((call, channel_for_inspection))
    }

    /// Join a call room: dial its current participants over the mesh, then
    /// publish our own join so future joiners can discover and dial us.
    pub async fn join_call_room(
        &self,
        space_id: Uuid,
        room: &crate::call_rooms::CallRoom,
    ) -> Result<(crate::channel::MeshActiveCall, crate::channel::MeshVoiceChannel)> {
        let endpoint = self.require_endpoint()?.clone();
        let my_id = endpoint.id().to_string();
        let author = self.space_author(space_id).await?;
        let blobs = self.require_blobs()?.clone();

        // let CallRoomProtocol::accept find our space when someone else dials into us later
        self.room_spaces.lock().unwrap().insert(room.id, space_id);

        let known_participants: Vec<String> = room
            .list_active_members(blobs)
            .await?
            .into_iter()
            .filter(|id| id != &my_id)
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let mut peers = Vec::new();
        for peer_id_str in known_participants {
            if let Ok(peer_id) = peer_id_str.parse::<iroh::EndpointId>() {
                peers.push((peer_id_str, peer_id.into()));
            }
        }

        let result = self.join_mesh(room.id, my_id.clone(), peers).await?;

        // publish our own join, so peers who join after us can discover and dial us
        room.set_membership(author, my_id.clone(), true).await?;

        let _ = self.events.send(crate::events::AppEvent::NewCallParticipant {
            space_id,
            room_id: room.id,
            endpoint_id: my_id,
        });

        Ok(result)
    }
}

pub async fn watch_doc<F, Fut>(doc: Doc, label: impl Into<String>, mut on_event: F) -> Result<JoinHandle<()>>
where
    F: Fn(LiveEvent) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let label = label.into();
    let events = doc.subscribe().await?;
    let handle = tokio::spawn(async move {
        let mut events = std::pin::pin!(events);
        while let Some(event) = events.next().await {
            match event {
                Ok(event) => on_event(event).await,
                Err(e) => eprintln!("[{label}] event error: {e}"),
            }
        }
    });

    Ok(handle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;
    use tempfile::tempdir;

    #[tokio::test]
    async fn watch_members_emits_new_member_event_for_new_entry() -> Result<()> {
        let identity = crate::identity::Identity::new("Alice");
        let mut node = Node::new(identity);
        let dir = tempdir().context("failed to create tempdir")?;
        node.run(dir.path().to_path_buf()).await?;

        let mut events = node.subscribe_events();

        let space = node.create_space("Test Space").await?;

        let docs = node.docs.as_ref().context("docs not created")?;
        let bob_author = docs.author_create().await?;
        let bob = crate::space::Member {
            author_id: bob_author.to_string(),
            endpoint_id: "bob-endpoint".to_string(),
            display_name: "Bob".to_string(),
            joined_at: 0,
            is_relay: false,
        };
        let key = format!("member/{bob_author}");
        let space_key = space.key().context("space has no key")?;
        let plaintext = postcard::to_allocvec(&bob).context("failed to encode member")?;
        let value = crate::crypto::encrypt(&space_key, &plaintext, space.id().as_bytes())
            .context("failed to encrypt member")?;
        space
            .members
            .set_bytes(bob_author, key.into_bytes(), value)
            .await?;

        let found = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                match events.recv().await {
                    Ok(AppEvent::NewMember { member, .. }) if member.display_name == "Bob" => {
                        return true;
                    }
                    Ok(_) => continue,
                    Err(_) => return false,
                }
            }
        })
        .await
        .unwrap_or(false);

        assert!(found, "expected a NewMember event with display_name \"Bob\"");
        Ok(())
    }

    #[tokio::test]
    async fn leaving_space_tombstones_member_and_emits_member_left_event() -> Result<()> {
        let identity = crate::identity::Identity::new("Alice");
        let mut node = Node::new(identity);
        let dir = tempdir().context("failed to create tempdir")?;
        node.run(dir.path().to_path_buf()).await?;

        let mut events = node.subscribe_events();
        let space = node.create_space("Test Space").await?;
        let author = space.author().context("author not created")?;

        space.leave(author).await?;

        let found = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                match events.recv().await {
                    Ok(AppEvent::MemberLeft { author_id, .. }) if author_id == author.to_string() => {
                        return true;
                    }
                    Ok(_) => continue,
                    Err(_) => return false,
                }
            }
        })
        .await
        .unwrap_or(false);

        assert!(found, "expected a MemberLeft event for the departing author");

        let members = space.member_cache.lock().await;
        assert!(members.iter().all(|m| m.author_id != author.to_string()));

        Ok(())
    }

    #[tokio::test]
    async fn node_leave_space_removes_it_from_spaces_list() -> Result<()> {
        let identity = crate::identity::Identity::new("Alice");
        let mut node = Node::new(identity);
        let dir = tempdir().context("failed to create tempdir")?;
        node.run(dir.path().to_path_buf()).await?;

        let space = node.create_space("Test Space").await?;
        let space_id = space.id();

        node.leave_space(space_id, dir.path().join("spaces")).await?;

        let spaces = node.spaces.lock().await;
        assert!(spaces.iter().all(|s| s.id() != space_id));
        Ok(())
    }
}
