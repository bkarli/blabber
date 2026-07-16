use std::path::PathBuf;
use iroh_docs::api::Doc;
use iroh_docs::engine::LiveEvent;
use n0_future::StreamExt;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use crate::events::AppEvent;
use crate::space::Space;
use crate::Identity;
use crate::channel::{VoiceChannel, VoiceProtocol, VOICE_ALPN};
use crate::invite::Invite;
use anyhow::{Result};
use iroh::{protocol::Router, Endpoint, SecretKey, EndpointId, endpoint::presets};
use iroh_blobs::store::fs::FsStore;
use iroh_docs::AuthorId;
use iroh_gossip::{api::Event, Gossip, TopicId};
use uuid::Uuid;
use iroh_blobs::{ALPN as BLOBS_ALPN, BlobsProtocol};
use iroh_gossip::{ALPN as GOSSIP_ALPN};
use iroh_docs::{protocol::Docs, ALPN as DOCS_ALPN};
use iroh_docs::api::protocol::ShareMode;

use anyhow::Context;
use tokio::fs;
use x25519_dalek::{EphemeralSecret, PublicKey};


pub struct Node {
    identity: Identity,
    pub endpoint: Option<Endpoint>,
    pub gossip: Option<Gossip>,
    pub router: Option<Router>,
    pub blobs: Option<FsStore>,
    pub docs: Option<Docs>,
    pub author: Option<AuthorId>,
    
    // Node can produce App Events and GUI can subscribe to these events
    pub events: broadcast::Sender<AppEvent>
}

impl Node {
    pub fn new(identity: Identity) -> Self {
        // create a broadcast channel
        let (events, _) = broadcast::channel(256);
        Self {
            identity,
            endpoint: None,
            gossip: None,
            router: None,
            blobs: None,
            docs: None,
            author: None,
            events,
        }
    }

    /// Create the endpoint from the identity
    /// This should always generate always the same Enpoint
    pub async fn create_endpoint(&mut self) -> Result<()> {
        let secret_key = SecretKey::from_bytes(&self.identity.secret);
        let ep = Endpoint::builder(presets::N0)
            .secret_key(secret_key)
            .alpns(vec![])
            .bind()
            .await?;

        // replace the endpoint in the strcut
        self.endpoint = Some(ep);
        Ok(())
    }

    pub async fn create_blobs(&mut self, path: &PathBuf) -> Result<()> {
        let blobs = FsStore::load(path).await?;
        self.blobs = Some(blobs);
        Ok(())
    }

    pub async fn create_gossip(&mut self) -> Result<()> {
        let endpoint = self
            .endpoint
            .clone()
            .context("endpoint not created yet")?;

        let gossip = Gossip::builder().spawn(endpoint);

        self.gossip = Some(gossip);
        Ok(())
    }

    pub async fn create_docs_engine(&mut self, path: &PathBuf) -> Result<()> {
        let endpoint = self
            .endpoint
            .clone()
            .context("endpoint not created yet")?;

        let gossip = self
            .gossip
            .clone()
            .context("gossip not created yet")?;

        let blobs = self
            .blobs
            .clone()
            .context("blobs not created yet")?;

        let docs = Docs::persistent(path.to_path_buf())
            .spawn(endpoint, blobs.into(), gossip)
            .await?;

        self.docs = Some(docs);
        Ok(())
    }

    pub async fn create_router(&mut self) -> Result<()> {
        let endpoint = self
            .endpoint
            .clone()
            .context("endpoint not created yet")?;

        let gossip = self
            .gossip
            .clone()
            .context("gossip not created yet")?;

        let blobs = self
            .blobs
            .clone()
            .context("blobs not created yet")?;

        let docs = self
            .docs
            .clone()
            .context("docs not created yet")?;

        let voice = VoiceProtocol::new();

        let router = Router::builder(endpoint)
            .accept(GOSSIP_ALPN, gossip)
            .accept(BLOBS_ALPN, BlobsProtocol::new(&blobs, None))
            .accept(DOCS_ALPN, docs)
            .accept(VOICE_ALPN, voice)
            .spawn();

        self.router = Some(router);
        Ok(())
    }

    pub async fn create_author(&mut self) -> Result<()> {
        let docs = self.docs.as_ref().context("Docs engine not created yet")?;
        let author = docs.author_create().await?;
        self.author = Some(author);
        Ok(())
    }

    pub async fn create_space(&self, name: impl Into<String>) -> Result<Space> {
        let docs = self.docs.as_ref().context("docs engine not created yet")?;
        let author = self.author.context("author not created yet")?;
        let endpoint = self.endpoint.as_ref().context("endpoint not created yet")?;
        
        let endpoint_id = endpoint.id().to_string();

        Space::new(docs,author, endpoint_id, self.identity.displayName.clone(), name).await
    }

    pub async fn join_space(&self, invite: Invite) -> Result<Space> {
        let docs = self.docs.as_ref().context("docs engine not created yet")?;
        let author = self.author.context("author not created yet")?;
        let endpoint = self.endpoint.as_ref().context("endpoint not created yet")?;
        let endpoint_id = endpoint.id().to_string();

        Space::from_invite(docs, invite, author, endpoint_id, self.identity.displayName.clone()).await
    }

    /// Additionally we need to load the docs
    pub async fn load_spaces(&mut self, root_path: PathBuf) -> Result<Vec<Space>> {
        // go through the root_path and enumerate all the spaces present
        // in the directory
        // root_directory
        //      - UUID
        //          - Meta
        //              - Info Read only
        //              - Members
        //              - Channels and Rooms
        //          - [UUID]chat
        
        let docs = self.docs.as_ref().context("docs engine not created yet")?;
        let author = self.author.context("author not created yet")?;
        let endpoint = self.endpoint.as_ref().context("endpoint not created yet")?;
        let endpoint_id = endpoint.id().to_string();
        let display_name = self.identity.displayName.clone();
        
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
            if Uuid::parse_str(dir_name).is_err(){
                continue;
            }
            let invite_path = path.join("meta").join("invite.txt");
            if !invite_path.is_file(){
                continue;
            }
            
            let code = fs::read_to_string(&invite_path).await?;
            let invite = match Invite::deserialize_invite(code){
                Ok(i) => i,
                Err(e) =>{
                    eprintln!("skipping unreadable space {dir_name}");
                    continue;
                }
            };
            let space = Space::from_invite(docs,invite,author, endpoint_id.clone(),display_name.clone(),).await?;
            spaces.push(space);
            }
        Ok(spaces)
    }
    

    /// Generic function on watching a Document
    pub async fn watch_doc<F, Fut>(&self, doc: Doc, label: impl Into<String>, mut on_event: F) -> Result<JoinHandle<()>>
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

    
    /// Run the endpoint
    ///
    /// listen for incoming gossip connections
    /// listen for incoming Voice connections
    pub async fn run(&mut self, blobs_path: PathBuf) -> Result<()> {
        self.create_endpoint().await?;
        // run wait online only in tests
        #[cfg(test)]
        {
            self.wait_online().await?;
        }
        self.create_gossip().await?;
        self.create_blobs(&blobs_path).await?;
        self.create_docs_engine(&blobs_path).await?;
        self.create_author().await?;
        self.create_router().await?;
        Ok(())
    }

    
    /// test function to keep the node online
    pub async fn wait_online(&self) -> Result<()> {
        use tokio::time::{timeout, Duration};

        let endpoint = self.endpoint.as_ref().context("endpoint not created yet")?;

        timeout(Duration::from_secs(5), endpoint.online())
            .await
            .context("timed out waiting for endpoint to come online")?;

        Ok(())
    }

    // get a receiver for app-level events
    pub fn subscribe_events(&self) -> broadcast::Receiver<AppEvent> {
        self.events.subscribe()
    }

    pub async fn call(&self, peer: impl Into<iroh::EndpointAddr>)->Result<crate::channel::ActiveVoiceCall> {
        let endpoint = self.endpoint.clone().context("Node not created yet")?;
        let connection = endpoint.connect(peer, VOICE_ALPN).await.context("failed to connect to voice call")?;
        let key = perform_key_exchange_as_initiator(&connection).await?;        
        let channel = VoiceChannel::new(connection, key);
        let handle = tokio::runtime::Handle::current();
        Ok(crate::channel::ActiveVoiceCall::start(channel, handle))
    }

}

async fn diffie_hellman(send: &mut iroh::endpoint::SendStream,recv: &mut iroh::endpoint::RecvStream) -> Result<[u8; 32]> {
    let my_secret = EphemeralSecret::random_from_rng(rand_core::OsRng);
    let my_public = PublicKey::from(&my_secret);

    send.write_all(my_public.as_bytes()).await?;
    send.finish()?;

    let mut their_public_bytes = [0u8; 32];
    recv.read_exact(&mut their_public_bytes).await?;
    let their_public = PublicKey::from(their_public_bytes);

    let shared_secret = my_secret.diffie_hellman(&their_public);
    Ok(*shared_secret.as_bytes())
}

pub async fn perform_key_exchange_as_initiator(connection: &iroh::endpoint::Connection) -> Result<[u8; 32]> {
    let (mut send, mut recv) = connection.open_bi().await?;
    diffie_hellman(&mut send, &mut recv).await
}

pub async fn perform_key_exchange_as_acceptor(connection: &iroh::endpoint::Connection) -> Result<[u8; 32]> {
    let (mut send, mut recv) = connection.accept_bi().await?;
    diffie_hellman(&mut send, &mut recv).await
}
    

#[cfg(test)]
mod tests {

    use super::*;
    use crate::channel::VOICE_ALPN;
    use iroh::endpoint::presets;
    use iroh::Endpoint;
    use anyhow::Context;
    use tempfile::tempdir;
    
    #[tokio::test]
    async fn test_create_endpoint() {
        let identity = Identity::new("Alice");

        let mut node = Node::new(identity);
        let result = node.create_endpoint().await;

        assert!(result.is_ok());
        assert!(node.endpoint.is_some());
    }

    #[tokio::test]
    async fn test_create_router() {
        let identity = Identity::new("Alice");
        let mut node = Node::new(identity);

        node.create_endpoint().await.unwrap();
        node.create_gossip().await.unwrap();

        let tmp = tempdir().unwrap();
        node.create_blobs(&tmp.path().to_path_buf()).await.unwrap();
        node.create_docs_engine(&tmp.path().to_path_buf()).await.unwrap();

        let result = node.create_router().await;

        assert!(result.is_ok());
        assert!(node.router.is_some());
    }

    #[tokio::test]
    async fn test_create_and_join_space() {
        let identity_a = Identity::new("Alice");
        let mut node_a = Node::new(identity_a);
        let tmp_a = tempdir().unwrap();
        node_a.run(tmp_a.path().to_path_buf()).await.unwrap();

        let space_a = node_a.create_space("Test Space").await.unwrap();
        let invite = space_a.create_invite().await.unwrap();

        let code = invite.serialize_invite().unwrap();
        let invite = Invite::deserialize_invite(code).unwrap();

        let identity_b = Identity::new("Bob");
        let mut node_b = Node::new(identity_b);
        let tmp_b = tempdir().unwrap();
        node_b.run(tmp_b.path().to_path_buf()).await.unwrap();

        let space_b = node_b.join_space(invite).await.unwrap();

        assert_eq!(space_b.name(), space_a.name());
        assert_eq!(space_b.id(), space_a.id());
    }

    #[tokio::test]
    async fn test_document_subscribe() {}

        

    #[tokio::test]
    async fn test_dh_key_exchange_produces_matching_keys() -> Result<()> {
        let endpoint_a = Endpoint::builder(presets::N0)
            .alpns(vec![VOICE_ALPN.to_vec()])
            .bind()
            .await?;
        let endpoint_b = Endpoint::builder(presets::N0)
            .alpns(vec![VOICE_ALPN.to_vec()])
            .bind()
            .await?;

        let addr_b = endpoint_b.addr();
        let endpoint_b_for_accept = endpoint_b.clone();
        let accept_task = tokio::spawn(async move {
            let incoming = endpoint_b_for_accept
                .accept()
                .await
                .context("no incoming connection")?;
            let connection = incoming.await.context("failed to accept connection")?;
            Ok::<_, anyhow::Error>(connection)
        });

        let connection_a = endpoint_a
            .connect(addr_b, VOICE_ALPN)
            .await
            .context("A failed to connect to B")?;
        let connection_b = accept_task.await.context("accept task panicked")??;

        let (key_a, key_b) = tokio::try_join!(
            perform_key_exchange_as_initiator(&connection_a),
            perform_key_exchange_as_acceptor(&connection_b),
        )?;
        assert_eq!(key_a, key_b);
        Ok(())
    }
    
    #[tokio::test]
    async fn test_room_creation_and_message_sync() {
        use tokio::time::{sleep, timeout, Duration};

        let identity_a = Identity::new("Alice");
        let mut node_a = Node::new(identity_a);
        let tmp_a = tempdir().unwrap();
        node_a.run(tmp_a.path().to_path_buf()).await.unwrap();

        let space_a = node_a.create_space("Test Space").await.unwrap();

        let docs_a = node_a.docs.as_ref().unwrap();
        let author_a = node_a.author.unwrap();

        space_a.create_room(docs_a, author_a, "general").await.unwrap();

        let rooms_a = space_a.rooms.lock().await;
        let room_a = rooms_a.first().unwrap();
        room_a.send_message(author_a, "hello from alice").await.unwrap();
        drop(rooms_a); 

        let invite = space_a.create_invite().await.unwrap();
        let code = invite.serialize_invite().unwrap();
        let invite = Invite::deserialize_invite(code).unwrap();

        let identity_b = Identity::new("Bob");
        let mut node_b = Node::new(identity_b);
        let tmp_b = tempdir().unwrap();
        node_b.run(tmp_b.path().to_path_buf()).await.unwrap();

        let space_b = node_b.join_space(invite).await.unwrap();

        let docs_b = node_b.docs.as_ref().unwrap();
        let blobs_b = node_b.blobs.as_ref().unwrap();

        let found = timeout(Duration::from_secs(15), async {
        loop {
            let handles = space_b.sync_rooms(&node_b, docs_b, blobs_b).await;
            match &handles {
                Ok(h) => eprintln!("sync_rooms ok, {} handles", h.len()),
                Err(e) => eprintln!("sync_rooms error: {e:#}"),
            }

            let rooms_b = space_b.rooms.lock().await;
            eprintln!("known rooms on B: {:?}", rooms_b.iter().map(|r| r.name.clone()).collect::<Vec<_>>());

            if let Some(room_b) = rooms_b.iter().find(|r| r.name == "general") {
                let cache = room_b.cache.lock().await;
                eprintln!("room 'general' cache: {:?}", cache.iter().map(|m| &m.content).collect::<Vec<_>>());
                if cache.iter().any(|m| m.content == "hello from alice") {
                    return;
                }
            }
            drop(rooms_b);
            sleep(Duration::from_millis(1000)).await; // slower interval so the log isn't flooded
        }
        })
        .await;
        assert!(found.is_ok(), "Bob never discovered the room or received Alice's message");
    }

    #[tokio::test]
    async fn test_broadcast_emits_new_room_event_local() {
        let id = Identity::new("Alice");
        let mut node = Node::new(id);
        
        // create a temporary directory
        let tmp = tempdir().unwrap();
        node.run(tmp.path().to_path_buf()).await.unwrap();


    }


    #[tokio::test]
    async fn test_broadcast_emits_new_message_event_local() { 

    }


    #[tokio::test]
    async fn test_broadcast_emits_new_room_event() { 

    }


    #[tokio::test]
    async fn test_broadcast_emits_new_message_event() { 

    }

}
