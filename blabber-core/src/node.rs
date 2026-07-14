use crate::Identity;
use anyhow::{Result};
use iroh::{protocol::Router, Endpoint, SecretKey, EndpointId, endpoint::presets};
use iroh_gossip::{api::Event, Gossip, TopicId};

pub struct Node {
    identity: Identity,
    pub endpoint: Option<Endpoint>
}

impl Node {
    pub fn new(identity: Identity) -> Self {
        Self {
            identity,
            endpoint: None,
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
        
        let gossip = Gossip::builder()
            .spawn(self.endpoint.clone().expect("Node not created yet"));

        let router = Router::builder(self.endpoint.clone().expect("Node not created yet"))
            .accept(iroh_gossip::ALPN, gossip.clone())
            .spawn();

        // Do the listening

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
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

}
