use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use blabber_core::identity::Identity;
use blabber_core::invite::RelayInvite;
use zeroize::Zeroizing;

use crate::config::render_config;

const DEFAULT_DISPLAY_NAME: &str = "blabber-root";
const DEFAULT_DATA_DIR: &str = "/var/lib/blabber-root";
const DEFAULT_RUN_AS_USER: &str = "blabber-root";
const SYSTEMD_CREDENTIAL_PATH: &str = "/run/credentials/blabber-root.service/password";

/// Interactively collects display name, data dir, password, and invite
/// tickets, then writes a valid `config.toml` plus a password file with
/// sane permissions. Scope is intentionally limited to those two files -
/// creating the system user and installing the systemd unit stay manual,
/// one-time, documented steps.
pub fn run(config_path: &Path) -> Result<()> {
    if config_path.exists()
        && !confirm(
            &format!(
                "Config already exists at {}. Overwrite?",
                config_path.display()
            ),
            false,
        )?
    {
        println!("leaving existing config in place");
        return Ok(());
    }

    let config_dir = config_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    let display_name = prompt_with_default("Display name for this relay", DEFAULT_DISPLAY_NAME)?;
    let data_dir = PathBuf::from(prompt_with_default("Data directory", DEFAULT_DATA_DIR)?);
    let password_file = provision_password(config_dir, &data_dir)?;
    let invites = collect_invites()?;

    fs::create_dir_all(config_dir)
        .with_context(|| format!("creating config directory {}", config_dir.display()))?;
    let rendered = render_config(&display_name, &data_dir, &password_file, &invites);
    fs::write(config_path, rendered)
        .with_context(|| format!("writing config to {}", config_path.display()))?;

    println!("\nwrote config to {}", config_path.display());
    println!("start (or restart) the service to pick it up:");
    println!("  sudo systemctl restart blabber-root");
    Ok(())
}

/// Asks how the identity password should be provisioned, writes it to disk
/// with mode 0600, and returns the path `config.toml`'s `password_file`
/// should point at.
fn provision_password(config_dir: &Path, data_dir: &Path) -> Result<PathBuf> {
    let use_systemd_credential = confirm(
        "Use systemd credential provisioning (recommended - password never touches disk readable by the service account)?",
        true,
    )?;

    let mut password = read_password_twice()?;

    let identity_path = data_dir.join("identity.bin");
    if identity_path.exists() {
        password = verify_or_reenter_password(&identity_path, password)?;
    }

    if use_systemd_credential {
        let source_path = config_dir.join("password.source");
        write_secret_file(&source_path, &password)?;
        println!(
            "wrote {} (root-only). The unit's `LoadCredential=password:{}` line exposes it to the service at {}.",
            source_path.display(),
            source_path.display(),
            SYSTEMD_CREDENTIAL_PATH
        );
        Ok(PathBuf::from(SYSTEMD_CREDENTIAL_PATH))
    } else {
        let default_path = config_dir.join("password");
        let path = PathBuf::from(prompt_with_default(
            "Path to write the password file",
            &default_path.display().to_string(),
        )?);
        write_secret_file(&path, &password)?;

        let run_as_user = prompt_with_default("System user the service runs as", DEFAULT_RUN_AS_USER)?;
        chown_best_effort(&path, &run_as_user);

        Ok(path)
    }
}

/// An identity already exists at `identity_path` from a previous run. Checks
/// the freshly-entered password actually decrypts it (the same check
/// `main.rs` does at startup) before we let a mismatched password get
/// written out, since `setup` never touches existing identity material.
fn verify_or_reenter_password(
    identity_path: &Path,
    mut password: Zeroizing<String>,
) -> Result<Zeroizing<String>> {
    loop {
        if Identity::load_from_disk(identity_path, &password).is_ok() {
            return Ok(password);
        }

        eprintln!(
            "\nwarning: this password does not decrypt the existing identity at {}",
            identity_path.display()
        );
        let choice = prompt_with_default(
            "[r]e-enter the correct password / [d]elete it and start fresh under a new identity / [c]ontinue anyway (service won't start until this is fixed)",
            "r",
        )?;

        match choice.chars().next().map(|c| c.to_ascii_lowercase()) {
            Some('d') => {
                if confirm(
                    &format!("Delete {}? This is irreversible.", identity_path.display()),
                    false,
                )? {
                    fs::remove_file(identity_path).with_context(|| {
                        format!("deleting identity {}", identity_path.display())
                    })?;
                    println!(
                        "deleted - a fresh identity (new relay keypair) will be created on next start"
                    );
                    return Ok(password);
                }
            }
            Some('c') => {
                eprintln!("continuing with a password that won't decrypt the existing identity");
                return Ok(password);
            }
            _ => {
                password = read_password_twice()?;
            }
        }
    }
}

fn chown_best_effort(path: &Path, user: &str) {
    let owner = format!("{user}:{user}");
    match Command::new("chown").arg(&owner).arg(path).status() {
        Ok(status) if status.success() => {
            println!("chowned {} to {owner}", path.display());
        }
        Ok(status) => eprintln!(
            "warning: `chown {owner} {}` exited with {status}; fix ownership manually before starting the service",
            path.display()
        ),
        Err(e) => eprintln!(
            "warning: could not run chown ({e:#}); fix ownership manually: sudo chown {owner} {}",
            path.display()
        ),
    }
}

fn write_secret_file(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating directory {}", parent.display()))?;
    }
    fs::write(path, contents).with_context(|| format!("writing {}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("setting permissions on {}", path.display()))?;
    Ok(())
}

fn read_password_twice() -> Result<Zeroizing<String>> {
    loop {
        let first = Zeroizing::new(
            rpassword::prompt_password("Identity password: ").context("reading password")?,
        );
        if first.is_empty() {
            eprintln!("password cannot be empty");
            continue;
        }
        let second = Zeroizing::new(
            rpassword::prompt_password("Confirm password: ")
                .context("reading password confirmation")?,
        );
        if *first != *second {
            eprintln!("passwords did not match, try again");
            continue;
        }
        return Ok(first);
    }
}

fn collect_invites() -> Result<Vec<String>> {
    let mut invites = Vec::new();
    if !confirm("Add a relay invite ticket now?", false)? {
        return Ok(invites);
    }
    loop {
        let ticket = prompt("Paste ticket (blank to stop)")?;
        if ticket.is_empty() {
            break;
        }
        match RelayInvite::deserialize_invite(ticket.clone()) {
            Ok(_) => {
                invites.push(ticket);
                println!("ticket accepted ({} so far)", invites.len());
                if !confirm("Add another?", false)? {
                    break;
                }
            }
            Err(e) => {
                eprintln!("that doesn't look like a valid relay invite ticket: {e:#}");
            }
        }
    }
    Ok(invites)
}

fn prompt(label: &str) -> Result<String> {
    print!("{label}: ");
    io::stdout().flush().ok();
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .context("reading from stdin")?;
    Ok(line.trim().to_string())
}

fn prompt_with_default(label: &str, default: &str) -> Result<String> {
    print!("{label} [{default}]: ");
    io::stdout().flush().ok();
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .context("reading from stdin")?;
    let trimmed = line.trim();
    Ok(if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    })
}

fn confirm(label: &str, default_yes: bool) -> Result<bool> {
    let hint = if default_yes { "[Y/n]" } else { "[y/N]" };
    print!("{label} {hint}: ");
    io::stdout().flush().ok();
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .context("reading from stdin")?;
    Ok(match line.trim().to_lowercase().as_str() {
        "" => default_yes,
        "y" | "yes" => true,
        "n" | "no" => false,
        _ => default_yes,
    })
}
