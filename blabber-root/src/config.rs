// 
use std::path::PathBuf;
use anyhow::Result;


pub struct Config {
    secret: [u8; 32],
    invite: Option<()>
}


impl Config {
    // create a new config
    pub fn new(path: &PathBuf) -> Self {
        Self {
            secret: [0u8;32],
            invite: None,
        } 
    }
    
    // parse the config from file
    pub fn from_file(path: &PathBuf) -> Result<Self> {
        Ok( Self { 
                secret: [0u8; 32],
                invite: None,
            })
    }
}


