mod config;
mod setup;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use blabber_core::identity::Identity;
use blabber_core::invite::RelayInvite;
use blabber_core::node::Node;
use tokio::signal::unix::{signal, SignalKind};
use zeroize::Zeroizing;

use config::{Config, LoadOutcome};

const DEFAULT_CONFIG_PATH: &str = "/etc/blabber-root/config.toml";

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1).peekable();
    if args.peek().map(String::as_str) == Some("setup") {
        args.next();
        let config_path = parse_config_flag(args);
        return setup::run(&config_path);
    }
    let config_path = parse_config_flag(args);

    let config = match Config::load_or_init(&config_path)? {
        LoadOutcome::TemplateWritten(path) => {
            eprintln!(
                "no config found; wrote a template to {}.\nedit password_file and invites, then restart.",
                path.display()
            );
            std::process::exit(1);
        }
        LoadOutcome::Loaded(config) => config,
    };

    std::fs::create_dir_all(&config.data_dir)
        .with_context(|| format!("creating data_dir {}", config.data_dir.display()))?;

    let password = read_password_file(&config.password_file)?;

    let identity_path = config.data_dir.join("identity.bin");
    let identity = if identity_path.exists() {
        Identity::load_from_disk(&identity_path, &password)
            .context("failed to load identity (wrong password file contents?)")?
    } else {
        let identity = Identity::new(config.display_name.as_str());
        identity
            .store(&password, &identity_path)
            .context("failed to persist newly created identity")?;
        identity
    };

    let mut node = Node::new(identity);
    node.run(config.data_dir.join("blobs"))
        .await
        .context("failed to start node (endpoint/gossip/blobs/docs/router)")?;

    let spaces_root = config.data_dir.join("spaces");
    tokio::fs::create_dir_all(&spaces_root).await?;
    let loaded = node
        .load_spaces(spaces_root.clone())
        .await
        .context("failed to load previously known spaces")?;
    println!("loaded {} previously known space(s)", loaded.len());

    join_new_invites(&node, &spaces_root, &config.invites).await;

    log_events(&node);

    run_until_shutdown(&node, &config_path, &spaces_root).await?;

    node.shutdown().await.context("failed to shut down cleanly")?;
    println!("shutdown complete");
    Ok(())
}

fn parse_config_flag(mut args: impl Iterator<Item = String>) -> PathBuf {
    match args.next().as_deref() {
        Some("--config") => args.next().map(PathBuf::from).unwrap_or_else(|| {
            eprintln!("--config requires a path");
            std::process::exit(2);
        }),
        Some(other) => {
            eprintln!("unknown argument: {other}");
            std::process::exit(2);
        }
        None => PathBuf::from(DEFAULT_CONFIG_PATH),
    }
}

fn read_password_file(path: &Path) -> Result<Zeroizing<String>> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("reading password file {}", path.display()))?;
    Ok(Zeroizing::new(raw.trim_end_matches(['\n', '\r']).to_string()))
}

async fn join_new_invites(node: &Node, spaces_root: &Path, invites: &[String]) {
    for ticket in invites {
        if let Err(e) = try_join_invite(node, spaces_root, ticket).await {
            eprintln!("failed to join invite (skipping): {e:#}");
        }
    }
}

async fn try_join_invite(node: &Node, spaces_root: &Path, ticket: &str) -> Result<()> {
    let invite = RelayInvite::deserialize_invite(ticket.trim().to_string())?;

    {
        let spaces = node.spaces.lock().await;
        if spaces.iter().any(|space| space.id() == invite.space_id) {
            return Ok(());
        }
    }

    let space = node.join_space_relay(invite).await?;

    let user_root = node.identity_scoped_path(&spaces_root.to_path_buf())?;
    space
        .create_directory(&user_root, &node.local_storage_key())
        .await?;

    let blobs = node.blobs.clone().context("blobs not created yet")?;
    space.sync_rooms(node, &blobs).await?;
    space.sync_call_rooms(&blobs).await?;

    println!("joined space '{}' ({})", space.name(), space.id());
    Ok(())
}

fn log_events(node: &Node) {
    let mut rx = node.subscribe_events();
    tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            println!("event: {event:?}");
        }
    });
}

async fn run_until_shutdown(node: &Node, config_path: &Path, spaces_root: &Path) -> Result<()> {
    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sighup = signal(SignalKind::hangup())?;

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("received SIGINT, shutting down");
                return Ok(());
            }
            _ = sigterm.recv() => {
                println!("received SIGTERM, shutting down");
                return Ok(());
            }
            _ = sighup.recv() => {
                println!("received SIGHUP, reloading config");
                match Config::load_or_init(config_path) {
                    Ok(LoadOutcome::Loaded(config)) => {
                        join_new_invites(node, spaces_root, &config.invites).await;
                    }
                    Ok(LoadOutcome::TemplateWritten(_)) => {
                        eprintln!("config file disappeared; wrote a fresh template, but keeping running with existing spaces");
                    }
                    Err(e) => eprintln!("failed to reload config, keeping previous state: {e:#}"),
                }
            }
        }
    }
}
