use std::path::PathBuf;
use iroh_docs::api::Doc;
use iroh_docs::engine::LiveEvent;
use n0_future::StreamExt;
use tokio::task::JoinHandle;

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
}

impl Node {
    pub fn new(identity: Identity) -> Self {
        Self {
            identity,
            endpoint: None,
            gossip: None,
            router: None,
            blobs: None,
            docs: None,
            author: None,
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
    pub async fn load_spaces(&mut self, root_path: PathBuf) -> Result<()> {
        // go through the root_path and enumerate all the spaces present
        // in the directory
        // root_directory
        //      - UUID
        //          - Meta
        //              - Info Read only
        //              - Members
        //              - Channels and Rooms
        //          - [UUID]chat
        

        let mut entries = fs::read_dir(root_path).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            if !path.is_dir() {
                continue;
            }

            let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };

            let Ok(space_id) = Uuid::parse_str(dir_name) else {
                continue;
            };

            let meta_path = path.join("meta");
            if meta_path.is_dir() {
                // get the info ticket
                // get the member ticket
                // get the channel and room information
            }
        }

        Ok(())
    }

    /// Generic function on watching a Document
    pub fn watch_doc<F>(&self, doc: Doc, label: &'static str, mut on_event: F) -> JoinHandle<()>
    where
        F: FnMut(LiveEvent) + Send + 'static,
    {
        tokio::spawn(async move {
            let events = match doc.subscribe().await {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("[{label}] failed to subscribe: {e}");
                    return;
                }
            };
            let mut events = std::pin::pin!(events);

            while let Some(event) = events.next().await {
                match event {
                    Ok(event) => on_event(event),
                    Err(e) => eprintln!("[{label}] event error: {e}"),
                }
            }
        })
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

    
    /// Run the endpoint
    ///
    /// listen for incoming gossip connections
    /// listen for incoming Voice connections
    pub async fn run(&mut self, blobs_path: PathBuf) -> Result<()> {
        self.create_endpoint().await?;
        self.create_gossip().await?;
        self.create_blobs(&blobs_path).await?;
        self.create_docs_engine(&blobs_path).await?;
        self.create_author().await?;
        self.create_router().await?;
        Ok(())
    }

    //passt nonig zum tauri command (no ahluege)
    pub async fn call(&self, peer: impl Into<iroh::EndpointAddr>)->Result<(cpal::Stream, cpal::Stream)>{
        let endpoint = self.endpoint.clone().context("Node not created yet")?;
        let connection = endpoint.connect(peer, VOICE_ALPN).await.context("failed to connect to voice call")?;
        let key = perform_key_exchange_as_initiator(&connection).await?;        
        let channel = VoiceChannel::new(connection, key);
        let capture_stream = channel.start_capture()?;
        let handle = tokio::runtime::Handle::current();
        let playback_stream = channel.start_playback(&handle);
        Ok((capture_stream, playback_stream?))
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
    use tempfile::tempdir;
    use crate::channel::VOICE_ALPN;
    use iroh::endpoint::presets;
    use iroh::Endpoint;
    use anyhow::Context;
    
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
    async fn test_watch_doc_fires_on_local_insert() {
        use std::sync::{Arc, Mutex};
        use tokio::time::{sleep, timeout, Duration};

        let identity = Identity::new("Alice");
        let mut node = Node::new(identity);
        let tmp = tempdir().unwrap();
        node.run(tmp.path().to_path_buf()).await.unwrap();

        let space = node.create_space("Test Space").await.unwrap();

        let received: Arc<Mutex<Vec<LiveEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let received_clone = received.clone();

        let handle = node.watch_doc(space.info.clone(), "test", move |event| {
            received_clone.lock().unwrap().push(event);
        });

        // give the subscription a moment to actually attach before we write
        sleep(Duration::from_millis(100)).await;

        let author = node.author.unwrap();
        space
            .info
            .set_bytes(author, b"greeting".to_vec(), b"hello".to_vec())
            .await
            .unwrap();

        let saw_insert = timeout(Duration::from_secs(5), async {
            loop {
                let events = received.lock().unwrap();
                if events
                    .iter()
                    .any(|e| matches!(e, LiveEvent::InsertLocal { .. }))
                {
                    return;
                }
                drop(events);
                sleep(Duration::from_millis(50)).await;
            }
        })
        .await;

        handle.abort();

        assert!(saw_insert.is_ok(), "watch_doc never observed the local insert");
    }
    }
