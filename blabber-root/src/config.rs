use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

/// Escapes a string for use inside a TOML basic string (the part between the quotes).
fn toml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\u{:04x}", c as u32))
            }
            c => out.push(c),
        }
    }
    out
}

fn toml_quoted(s: &str) -> String {
    format!("\"{}\"", toml_escape(s))
}

/// Renders a `config.toml`, quoting/escaping every value so the result is always
/// valid TOML regardless of what the caller passes (e.g. a hand-pasted invite
/// ticket or a path with unusual characters).
pub fn render_config(
    display_name: &str,
    data_dir: &Path,
    password_file: &Path,
    invites: &[String],
) -> String {
    let invites_toml = if invites.is_empty() {
        "[]".to_string()
    } else {
        let items: Vec<String> = invites
            .iter()
            .map(|ticket| format!("    {},", toml_quoted(ticket)))
            .collect();
        format!("[\n{}\n]", items.join("\n"))
    };

    format!(
        r#"# blabber-root config -- edit before starting the service.

# Display name for this node's identity. blabber-root never joins a space
# as a Member - it's a blind relay that can't decrypt content - but this
# name IS shown in the member list, clearly labeled as a relay, so members
# can see it's attached to the space. Also used locally in log output.
display_name = {display_name}

# Root directory for identity.bin, blobs/, and spaces/.
data_dir = {data_dir}

# File whose contents is the identity password. Provision this
# yourself and chmod 600 it, or point it at a systemd LoadCredential path.
# See blabber-root/README.md for the options.
password_file = {password_file}

# Relay invite ticket strings to auto-join at startup and on SIGHUP reload.
# Get one from the desktop app's "Get relay invite" action, not the regular
# "Get invite" action, whose ticket carries the space's decryption key and
# will be rejected here.
invites = {invites_toml}
"#,
        display_name = toml_quoted(display_name),
        data_dir = toml_quoted(&data_dir.display().to_string()),
        password_file = toml_quoted(&password_file.display().to_string()),
    )
}

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
        let template = render_config(
            "blabber-root",
            Path::new("/var/lib/blabber-root"),
            Path::new("/etc/blabber-root/password"),
            &[],
        );
        std::fs::write(path, template)
            .with_context(|| format!("writing config template to {}", path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_valid_toml_with_no_invites() {
        let rendered = render_config(
            "blabber-root",
            Path::new("/var/lib/blabber-root"),
            Path::new("/etc/blabber-root/password"),
            &[],
        );
        let config: Config = toml::from_str(&rendered).expect("template must parse");
        assert_eq!(config.display_name, "blabber-root");
        assert_eq!(config.data_dir, PathBuf::from("/var/lib/blabber-root"));
        assert_eq!(
            config.password_file,
            PathBuf::from("/etc/blabber-root/password")
        );
        assert!(config.invites.is_empty());
    }

    #[test]
    fn escapes_invite_tickets_containing_quotes_and_backslashes() {
        let tricky_invite = r#"weird"ticket\with\backslashes"#.to_string();
        let rendered = render_config(
            "blabber-root",
            Path::new("/var/lib/blabber-root"),
            Path::new("/etc/blabber-root/password"),
            &[tricky_invite.clone(), "second-ticket".to_string()],
        );
        let config: Config = toml::from_str(&rendered).expect("template with invites must parse");
        assert_eq!(
            config.invites,
            vec![tricky_invite, "second-ticket".to_string()]
        );
    }

    #[test]
    fn escapes_display_name_with_special_characters() {
        let rendered = render_config(
            "my \"relay\" \\ node",
            Path::new("/var/lib/blabber-root"),
            Path::new("/etc/blabber-root/password"),
            &[],
        );
        let config: Config = toml::from_str(&rendered).expect("template must parse");
        assert_eq!(config.display_name, "my \"relay\" \\ node");
    }
}
