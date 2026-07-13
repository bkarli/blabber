use crate::Identity;
use anyhow::{Result};

pub struct Node {
    identity: Identity
}

impl Node {
    pub fn new(identity: Identity) -> Self {
        Self {
            identity,
        }
    }

    pub fn create_endpoint(&self) -> Result<()> {
        Ok(())
    }
}

pub struct PeerNode {}


