//! User configuration: policy, limits, and handler registrations.
//!
//! This lives in core rather than in the CLI because **two** processes must
//! read it identically. When the daemon carried its own defaults instead,
//! a user who had configured `"dangerous": "deny"` still got a promptable
//! dialog, and configured terminal/ssh handlers silently vanished — the
//! divergence was invisible precisely because both halves looked correct on
//! their own. One loader, one set of semantics, no drift.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::dispatch::OpenerConfig;
use crate::error::{Error, Result};
use crate::limits::Limits;
use crate::policy::Policy;

/// `<config>/config.json` — policy and limit overrides. Absent means
/// defaults; malformed is an error, never a silent fallback to defaults
/// (a typo in a policy file must not quietly loosen it).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UserConfig {
    pub limits: Option<Limits>,
    pub policy: Option<Policy>,
}

impl UserConfig {
    pub fn load(path: &Path) -> Result<UserConfig> {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| Error::Manifest(format!("malformed {}: {e}", path.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(UserConfig::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn limits(&self) -> Limits {
        self.limits.clone().unwrap_or_default()
    }

    pub fn policy(&self) -> Policy {
        self.policy.clone().unwrap_or_default()
    }
}

/// `<config>/handlers.json` — how kinds map to local programs, managed by
/// `murl handler ...`. Manifests can never write this file; that asymmetry
/// is the point (see `docs/security.md`).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HandlersFile {
    /// Override the platform opener (advanced; normally unset).
    pub open: Option<Vec<String>>,
    /// Terminal handler argv; `{target}` is the working directory.
    pub terminal: Option<Vec<String>>,
    /// ssh handler argv; `{target}` is the full `ssh://` URL.
    pub ssh: Option<Vec<String>>,
    /// remote-desktop handler argv; `{target}` is the `rdp://`/`vnc://` URL.
    pub remote_desktop: Option<Vec<String>>,
    /// Handlers for `custom:<name>` kinds.
    pub custom: BTreeMap<String, Vec<String>>,
}

impl HandlersFile {
    pub fn load(path: &Path) -> Result<HandlersFile> {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| Error::Manifest(format!("malformed {}: {e}", path.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(HandlersFile::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    /// Apply these registrations on top of the platform defaults.
    pub fn to_opener(&self, os: &str, home_dir: Option<std::path::PathBuf>) -> OpenerConfig {
        let mut opener = OpenerConfig::platform_default(os, home_dir);
        if let Some(open) = &self.open {
            opener.open_argv = open.clone();
        }
        opener.terminal_argv = self.terminal.clone();
        opener.ssh_argv = self.ssh.clone();
        opener.remote_desktop_argv = self.remote_desktop.clone();
        opener.custom = self.custom.clone();
        opener
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::ConsentMode;

    fn temp(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("murl-cfg-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_files_are_defaults() {
        let dir = temp("missing");
        let cfg = UserConfig::load(&dir.join("nope.json")).unwrap();
        assert_eq!(cfg.policy().dangerous, ConsentMode::Prompt);
        assert!(HandlersFile::load(&dir.join("nope.json"))
            .unwrap()
            .terminal
            .is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn malformed_config_is_an_error_not_a_silent_default() {
        // A typo in a policy file must never quietly loosen the policy.
        let dir = temp("malformed");
        let path = dir.join("config.json");
        std::fs::write(&path, b"{not json").unwrap();
        assert!(UserConfig::load(&path).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn policy_and_handlers_round_trip() {
        let dir = temp("roundtrip");
        let path = dir.join("config.json");
        std::fs::write(
            &path,
            br#"{"policy":{"safe":"allow","sensitive":"prompt","dangerous":"deny"}}"#,
        )
        .unwrap();
        let cfg = UserConfig::load(&path).unwrap();
        assert_eq!(cfg.policy().safe, ConsentMode::Allow);
        assert_eq!(cfg.policy().dangerous, ConsentMode::Deny);

        let handlers = HandlersFile {
            terminal: Some(vec!["myterm".into(), "{target}".into()]),
            ssh: Some(vec!["myssh".into()]),
            ..HandlersFile::default()
        };
        let hpath = dir.join("handlers.json");
        handlers.save(&hpath).unwrap();
        let loaded = HandlersFile::load(&hpath).unwrap();
        let opener = loaded.to_opener("linux", None);
        assert_eq!(opener.terminal_argv, handlers.terminal);
        assert_eq!(opener.ssh_argv, handlers.ssh);
        assert_eq!(opener.open_argv, vec!["xdg-open".to_string()]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
