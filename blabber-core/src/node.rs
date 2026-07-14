use crate::Identity;
use crate::channel::{VoiceChannel, VoiceProtocol, VOICE_ALPN};
use anyhow::{Result};
use iroh::{protocol::Router, Endpoint, SecretKey, EndpointId, endpoint::presets};
use iroh_gossip::{api::Event, Gossip, TopicId};
use anyhow::Context;
use x25519_dalek::{EphemeralSecret, PublicKey};


pub struct Node {
    identity: Identity,
    pub endpoint: Option<Endpoint>,
    router: Option<Router>,
}

impl Node {
    pub fn new(identity: Identity) -> Self {
        Self {
            identity,
            endpoint: None,
            router: None,
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

    
    /// Run the endpoint
    ///
    /// listen for incoming gossip connections
    /// listen for incoming Voice connections
    pub async fn run(&mut self) -> Result<()> {    
        let endpoint = self.endpoint.clone().context("couldnt clone ep -> not created yet");        
        let gossip = Gossip::builder()
            .spawn(self.endpoint.clone().expect("Node not created yet"));
        let voice = VoiceProtocol::new();


        let router = Router::builder(self.endpoint.clone().expect("Node not created yet"))
            .accept(iroh_gossip::ALPN, gossip.clone()).accept(VOICE_ALPN,voice)
            .spawn();

        self.router = Some(router);
        Ok(())
    }

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
    async fn test_same_id_same_ep(){
        
    }

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

}
