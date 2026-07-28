use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

const TEMPLATE_TOML: &str = r#"# blabber-root config -- edit before starting the service.

# How this node appears as a Member of every space it joins.
display_name = "blabber-root"

# Root directory for identity.bin, blobs/, and spaces/.
data_dir = "/var/lib/blabber-root"

# File whose contents is the identity password. Provision this
# yourself and chmod 600 it, or point it at a systemd LoadCredential path.
# See blabber-root/README.md for the options.
password_file = "/etc/blabber-root/password"

# Invite ticket strings to auto-join at startup and on SIGHUP reload. Get one
# from the desktop apps "Get invite" action.
invites = []
"#;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_display_name")]
    pub display_name: String,
    pub data_dir: PathBuf,
    pub password_file: PathBuf,
    #[serde(default)]
    pub invites: Vec<String>,
}

fn default_display_name() -> String {
    "blabber-root".to_string()
}

pub enum LoadOutcome {
    Loaded(Config),
    TemplateWritten(PathBuf),
}

impl Config {
    pub fn load_or_init(path: &Path) -> Result<LoadOutcome> {
        if !path.exists() {
            Self::write_template(path)?;
            return Ok(LoadOutcome::TemplateWritten(path.to_path_buf()));
        }

        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading config file {}", path.display()))?;
        let config: Config = toml::from_str(&raw)
            .with_context(|| format!("parsing config file {}", path.display()))?;
        Ok(LoadOutcome::Loaded(config))
    }

    fn write_template(path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating config directory {}", parent.display()))?;
        }
        std::fs::write(path, TEMPLATE_TOML)
            .with_context(|| format!("writing config template to {}", path.display()))?;
        Ok(())
    }
}
