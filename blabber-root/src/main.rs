

mod config;

use std::path::PathBuf;

use config::Config;

#[tokio::main]
async fn main() {
    // check the path and then run the node with the
    // config If there is no config create the config file
    
    let path = PathBuf::new();
    let Some(config) = Config::from_file(&path) else {
        let config = Config::new(&path);
    }

