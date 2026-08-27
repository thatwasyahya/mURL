//! Directory discovery for the daemon, matching the CLI's rules (XDG, with
//! `MURL_*_DIR` overrides) so both processes see the same stores. Divergence
//! here would mean `murl open` and a daemon activation resolving different
//! manifests — a correctness bug with security consequences.

use std::path::PathBuf;

use murl_core::error::{Error, Result};

pub struct DaemonPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub home: Option<PathBuf>,
}

impl DaemonPaths {
    pub fn discover() -> Result<DaemonPaths> {
        let home = if cfg!(windows) {
            std::env::var_os("USERPROFILE").map(PathBuf::from)
        } else {
            std::env::var_os("HOME").map(PathBuf::from)
        };
        let config_dir = pick(
            "MURL_CONFIG_DIR",
            "XDG_CONFIG_HOME",
            ".config/murl",
            home.as_deref(),
            "config",
        )?;
        let data_dir = pick(
            "MURL_DATA_DIR",
            "XDG_DATA_HOME",
            ".local/share/murl",
            home.as_deref(),
            "data",
        )?;
        let cache_dir = pick(
            "MURL_CACHE_DIR",
            "XDG_CACHE_HOME",
            ".cache/murl",
            home.as_deref(),
            "cache",
        )?;
        Ok(DaemonPaths {
            config_dir,
            data_dir,
            cache_dir,
            home,
        })
    }

    pub fn names_dir(&self) -> PathBuf {
        self.data_dir.join("names")
    }

    pub fn trust_file(&self) -> PathBuf {
        self.config_dir.join("trust.json")
    }

    pub fn manifest_cache_dir(&self) -> PathBuf {
        self.cache_dir.join("manifests")
    }
}

fn pick(
    explicit: &str,
    xdg: &str,
    home_suffix: &str,
    home: Option<&std::path::Path>,
    label: &str,
) -> Result<PathBuf> {
    let env_dir = |name: &str| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    env_dir(explicit)
        .or_else(|| env_dir(xdg).map(|p| p.join("murl")))
        .or_else(|| home.map(|h| h.join(home_suffix)))
        .ok_or_else(|| {
            Error::Io(std::io::Error::other(format!(
                "cannot determine a {label} directory; set MURL_{}_DIR",
                label.to_uppercase()
            )))
        })
}
