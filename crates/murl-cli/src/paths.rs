//! Platform directory discovery.
//!
//! Follows XDG on Linux, sensible equivalents elsewhere, and honors explicit
//! `MURL_CONFIG_DIR` / `MURL_DATA_DIR` / `MURL_CACHE_DIR` overrides — which
//! are also what the integration tests use to stay hermetic.

use std::path::PathBuf;

use murl_core::error::{Error, Result};

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Result<AppPaths> {
        let home = home_dir();
        let config_dir = env_dir("MURL_CONFIG_DIR")
            .or_else(|| platform_config(home.as_deref()))
            .ok_or_else(|| {
                Error::Io(other(
                    "cannot determine a config directory; set MURL_CONFIG_DIR",
                ))
            })?;
        let data_dir = env_dir("MURL_DATA_DIR")
            .or_else(|| platform_data(home.as_deref()))
            .ok_or_else(|| {
                Error::Io(other(
                    "cannot determine a data directory; set MURL_DATA_DIR",
                ))
            })?;
        let cache_dir = env_dir("MURL_CACHE_DIR")
            .or_else(|| platform_cache(home.as_deref()))
            .ok_or_else(|| {
                Error::Io(other(
                    "cannot determine a cache directory; set MURL_CACHE_DIR",
                ))
            })?;
        Ok(AppPaths {
            config_dir,
            data_dir,
            cache_dir,
        })
    }

    pub fn names_dir(&self) -> PathBuf {
        self.data_dir.join("names")
    }

    pub fn trust_file(&self) -> PathBuf {
        self.config_dir.join("trust.json")
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.json")
    }

    pub fn handlers_file(&self) -> PathBuf {
        self.config_dir.join("handlers.json")
    }

    pub fn default_key_file(&self) -> PathBuf {
        self.config_dir.join("keys").join("default.key.json")
    }

    pub fn manifest_cache_dir(&self) -> PathBuf {
        self.cache_dir.join("manifests")
    }
}

pub fn home_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    } else {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

fn env_dir(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn platform_config(home: Option<&std::path::Path>) -> Option<PathBuf> {
    if cfg!(windows) {
        env_dir("APPDATA").map(|p| p.join("murl"))
    } else if cfg!(target_os = "macos") {
        home.map(|h| h.join("Library/Application Support/murl"))
    } else {
        env_dir("XDG_CONFIG_HOME")
            .map(|p| p.join("murl"))
            .or_else(|| home.map(|h| h.join(".config/murl")))
    }
}

fn platform_data(home: Option<&std::path::Path>) -> Option<PathBuf> {
    if cfg!(windows) {
        env_dir("LOCALAPPDATA").map(|p| p.join("murl").join("data"))
    } else if cfg!(target_os = "macos") {
        home.map(|h| h.join("Library/Application Support/murl/data"))
    } else {
        env_dir("XDG_DATA_HOME")
            .map(|p| p.join("murl"))
            .or_else(|| home.map(|h| h.join(".local/share/murl")))
    }
}

fn platform_cache(home: Option<&std::path::Path>) -> Option<PathBuf> {
    if cfg!(windows) {
        env_dir("LOCALAPPDATA").map(|p| p.join("murl").join("cache"))
    } else if cfg!(target_os = "macos") {
        home.map(|h| h.join("Library/Caches/murl"))
    } else {
        env_dir("XDG_CACHE_HOME")
            .map(|p| p.join("murl"))
            .or_else(|| home.map(|h| h.join(".cache/murl")))
    }
}

fn other(msg: &str) -> std::io::Error {
    std::io::Error::other(msg)
}
