use std::path::PathBuf;
use std::sync::Arc;
use iroh_docs::api::Doc;
use iroh_docs::engine::LiveEvent;
use n0_future::StreamExt;
use tokio::sync::{Mutex, broadcast};
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
    pub events: broadcast::Sender<AppEvent>,
    pub spaces: Arc<Mutex<Vec<Space>>>,
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
            spaces: Arc::new(Mutex::new(Vec::new())),
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
        
        // create the author only if there is None in the identity
        let author = match self.identity.author {
            Some(existing) => existing,
            None => {
                let new_author = docs.author_create().await?;
                // set the newly created author in the identity
                self.identity.author = Some(new_author);
                new_author
            }
        };
        self.author = Some(author);
        Ok(())
    }

    pub async fn create_space(&self, name: impl Into<String>) -> Result<Space> {
        let docs = self.docs.as_ref().context("docs engine not created yet")?;
        let author = self.author.context("author not created yet")?;
        let endpoint = self.endpoint.as_ref().context("endpoint not created yet")?;
        
        let endpoint_id = endpoint.id().to_string();

        let space = Space::new(docs,author, endpoint_id, self.identity.displayName.clone(), name).await?;
        self.spaces.lock().await.push(space.clone());
        println!("CREATE SPACE ID: {}", space.id());
        Ok(space)

    }

    pub async fn join_space(&self, invite: Invite) -> Result<()> {
        let docs = self.docs.as_ref().context("docs engine not created yet")?;
        let author = self.author.context("author not created yet")?;
        let endpoint = self.endpoint.as_ref().context("endpoint not created yet")?;
        let endpoint_id = endpoint.id().to_string();

        let space = Space::from_invite(docs, invite, author, endpoint_id, self.identity.displayName.clone()).await?;
        self.spaces.lock().await.push(space);
        Ok(())
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
        let blobs = self
            .blobs
            .as_ref()
            .context("blobs not created yet")?;
        
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
            let space = Space::from_invite(
                docs,
                invite,
                author,
                endpoint_id.clone(),
                display_name.clone(),
            ).await?;


            space
                .sync_rooms(self, blobs)
                .await
                .map_err(|error| {
                    anyhow::anyhow!(
            "failed to sync rooms for space {dir_name}: {error}"
        )

                })?;
            spaces.push(space);
            }

        self.spaces.lock().await.extend(spaces.clone());
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
    }

    #[tokio::test]
    async fn test_create_router() {}

    #[tokio::test]
    async fn test_create_and_join_space() {
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
    }

    #[tokio::test]
    async fn test_broadcast_emits_new_room_event_local() {
        let id = Identity::new("Alice");
        let mut node = Node::new(id);
        
        // create a temporary directory
        let tmp = tempdir().unwrap();
        node.run(tmp.path().to_path_buf()).await.unwrap();

        node.create_space("Test").await.unwrap();



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
