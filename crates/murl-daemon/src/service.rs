//! `murl-daemon service` — install the per-user service unit so the daemon
//! is running when an activation arrives.
//!
//! This matters most on macOS, where a Launch Services activation has no
//! terminal and therefore needs the daemon's dialog to be reachable, but it
//! is the same convenience everywhere: a scheme handler is worth little if
//! the thing that answers it must be started by hand first.
//!
//! Scope discipline, matching `murl os`: **per-user only**. A systemd
//! *user* unit or a `LaunchAgent`, never a system unit or a `LaunchDaemon`.
//! The daemon holds no capability the user lacks, so installing it
//! system-wide would grant it one — and this command will not do that.

use std::path::{Path, PathBuf};

use murl_core::error::{Error, Result};

/// Where the unit file for this platform belongs.
pub fn unit_path() -> Result<PathBuf> {
    let home =
        home_dir().ok_or_else(|| Error::Dispatch("cannot determine the home directory".into()))?;
    if cfg!(target_os = "macos") {
        Ok(home.join("Library/LaunchAgents/dev.murl.daemon.plist"))
    } else if cfg!(unix) {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        Ok(base.join("systemd/user/murl-daemon.service"))
    } else {
        Err(Error::Dispatch(
            "no service integration on this platform yet; run `murl-daemon run` directly".into(),
        ))
    }
}

/// Render the unit for this platform, pointing at `exe`.
pub fn unit_contents(exe: &Path) -> Result<String> {
    let exe = exe
        .to_str()
        .ok_or_else(|| Error::Dispatch("executable path is not valid UTF-8".into()))?;
    if cfg!(target_os = "macos") {
        // A plist is XML: a path containing `<` or `&` would break the
        // document. Refuse rather than emit something malformed.
        if exe.contains(['<', '>', '&', '"']) {
            return Err(Error::Dispatch(
                "executable path contains characters unsafe for a plist".into(),
            ));
        }
        Ok(format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>dev.murl.daemon</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe}</string>
        <string>run</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>ProcessType</key>
    <string>Interactive</string>
</dict>
</plist>
"#
        ))
    } else {
        // systemd values are not shell, but ExecStart splits on spaces
        // unless quoted, so a path with a space needs the quotes.
        if exe.contains('\n') || exe.contains('"') {
            return Err(Error::Dispatch(
                "executable path contains characters unsafe for a unit file".into(),
            ));
        }
        Ok(format!(
            r#"[Unit]
Description=mURL resolver (consent surface for murl:// activations)
Documentation=https://github.com/thatwasyahya/mURL
After=graphical-session.target

[Service]
Type=simple
ExecStart="{exe}" run
Restart=on-failure
RestartSec=2

[Install]
WantedBy=default.target
"#
        ))
    }
}

pub fn install() -> Result<i32> {
    let exe = std::env::current_exe()?;
    let path = unit_path()?;
    let contents = unit_contents(&exe)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, contents)?;
    println!("wrote {}", path.display());
    println!();
    if cfg!(target_os = "macos") {
        println!("enable it:");
        println!("  launchctl load -w {}", path.display());
        println!();
        println!("macOS needs this: a Launch Services activation has no terminal, so");
        println!("consent depends on the daemon dialog being reachable.");
    } else {
        println!("enable it:");
        println!("  systemctl --user daemon-reload");
        println!("  systemctl --user enable --now murl-daemon.service");
        println!();
        println!("note: this is a *user* unit. The daemon runs as you and holds no");
        println!("capability you lack; installing it system-wide would give it one.");
    }
    Ok(0)
}

pub fn uninstall() -> Result<i32> {
    let path = unit_path()?;
    if path.exists() {
        std::fs::remove_file(&path)?;
        println!("removed {}", path.display());
    } else {
        println!("{} was not installed", path.display());
    }
    if cfg!(target_os = "macos") {
        println!(
            "if it is loaded, disable it too:  launchctl unload -w {}",
            path.display()
        );
    } else {
        println!("disable it with:  systemctl --user disable --now murl-daemon.service");
    }
    Ok(0)
}

pub fn status() -> Result<i32> {
    let path = unit_path()?;
    println!(
        "unit file: {} ({})",
        path.display(),
        if path.exists() { "present" } else { "absent" }
    );
    let socket = crate::socket::socket_path()?;
    println!(
        "socket:    {} ({})",
        socket.display(),
        if crate::client::probe(&socket) {
            "a daemon is answering"
        } else {
            "no daemon answering"
        }
    );
    Ok(0)
}

fn home_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    } else {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_names_the_executable_and_the_run_subcommand() {
        let contents = unit_contents(Path::new("/opt/murl/bin/murl-daemon")).unwrap();
        assert!(contents.contains("/opt/murl/bin/murl-daemon"));
        assert!(contents.contains("run"));
        // Per-user only: never a system unit or a LaunchDaemon.
        assert!(!contents.contains("WantedBy=multi-user.target"));
        assert!(!contents.to_lowercase().contains("launchdaemon"));
    }

    #[test]
    fn paths_with_spaces_stay_one_argument() {
        let contents = unit_contents(Path::new("/home/a b/murl-daemon")).unwrap();
        if cfg!(target_os = "macos") {
            assert!(contents.contains("<string>/home/a b/murl-daemon</string>"));
        } else {
            assert!(contents.contains("ExecStart=\"/home/a b/murl-daemon\" run"));
        }
    }

    #[test]
    fn structurally_unsafe_paths_are_refused() {
        // Better than emitting a malformed unit that fails at load time in
        // a way nobody traces back to here.
        let bad = if cfg!(target_os = "macos") {
            Path::new("/home/u/<evil>/murl-daemon")
        } else {
            Path::new("/home/u/\"evil\"/murl-daemon")
        };
        assert!(unit_contents(bad).is_err());
    }

    #[test]
    fn unit_path_is_user_scoped() {
        if let Ok(path) = unit_path() {
            let text = path.to_string_lossy().into_owned();
            assert!(!text.starts_with("/etc/"), "{text}");
            assert!(!text.starts_with("/Library/"), "{text}");
        }
    }
}
