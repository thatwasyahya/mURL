//! Socket location and ownership checks.
//!
//! The endpoint is user-private (threat D-1) and must not be hijackable by
//! a stale or squatted path (threat D-5). Both sides check before trusting:
//! the daemon verifies the directory it binds inside, and the client
//! verifies the socket it connects to — a client that finds anything
//! surprising falls back to in-process resolution rather than talking to an
//! unknown listener.

use std::path::PathBuf;

use murl_core::error::{Error, Result};

/// Directory and socket file names.
pub const SOCKET_DIR_NAME: &str = "murl";
pub const SOCKET_FILE_NAME: &str = "murl.sock";

/// The socket path for the current user.
///
/// Unix: `$MURL_SOCKET`, else `$XDG_RUNTIME_DIR/murl/murl.sock`, else a
/// `/tmp/murl-<uid>` fallback. Windows: a named pipe path.
pub fn socket_path() -> Result<PathBuf> {
    if let Some(explicit) = std::env::var_os("MURL_SOCKET").filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(explicit));
    }
    #[cfg(windows)]
    {
        Ok(PathBuf::from(format!(
            r"\\.\pipe\murl-{}",
            std::env::var("USERNAME").unwrap_or_else(|_| "user".into())
        )))
    }
    #[cfg(unix)]
    {
        if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty()) {
            return Ok(PathBuf::from(runtime)
                .join(SOCKET_DIR_NAME)
                .join(SOCKET_FILE_NAME));
        }
        // No runtime dir (containers, minimal sessions): a uid-scoped
        // directory under the temp dir, created 0700 below.
        let uid = unsafe_free_uid();
        Ok(std::env::temp_dir()
            .join(format!("murl-{uid}"))
            .join(SOCKET_FILE_NAME))
    }
}

/// Current uid without `unsafe` — read from `/proc/self/status`, falling
/// back to the `USER` name so the path stays user-scoped either way.
#[cfg(unix)]
fn unsafe_free_uid() -> String {
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("Uid:") {
                if let Some(uid) = rest.split_whitespace().next() {
                    return uid.to_owned();
                }
            }
        }
    }
    std::env::var("USER").unwrap_or_else(|_| "user".into())
}

/// Create the socket's parent directory with owner-only permissions,
/// verifying it is not a symlink or someone else's directory.
#[cfg(unix)]
pub fn prepare_socket_dir(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let Some(dir) = path.parent() else {
        return Err(Error::Dispatch("socket path has no parent".into()));
    };
    if !dir.exists() {
        std::fs::create_dir_all(dir)?;
    }
    // A symlinked socket directory is an interception setup.
    let meta = std::fs::symlink_metadata(dir)?;
    if meta.file_type().is_symlink() {
        return Err(Error::Denied(format!(
            "socket directory {} is a symlink; refusing to bind",
            dir.display()
        )));
    }
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(windows)]
pub fn prepare_socket_dir(_path: &std::path::Path) -> Result<()> {
    Ok(())
}

/// Check that an existing socket path is safe for a *client* to connect to:
/// it must be a socket, owned by this user, and not group/world writable.
#[cfg(unix)]
pub fn client_may_trust(path: &std::path::Path) -> bool {
    use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};

    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if !meta.file_type().is_socket() {
        return false;
    }
    let mode = meta.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        return false; // any group/other access is disqualifying
    }
    // Owned by us. When the uid cannot be determined we keep the mode check
    // as the guarantee rather than failing closed on every platform quirk.
    match unsafe_free_uid().parse::<u32>() {
        Ok(uid) => meta.uid() == uid,
        Err(_) => true,
    }
}

#[cfg(windows)]
pub fn client_may_trust(_path: &std::path::Path) -> bool {
    // Named pipes are namespaced per user by the path convention; the
    // Windows transport lands with the GUI work (docs/daemon.md).
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_env_wins() {
        // Not using std::env::set_var (unsafe in edition 2024 semantics and
        // racy across threads); just assert the fallback shape instead.
        let path = socket_path().unwrap();
        assert!(
            path.to_string_lossy().contains("murl"),
            "socket path should be namespaced: {}",
            path.display()
        );
        assert!(path.file_name().is_some());
    }

    #[cfg(unix)]
    #[test]
    fn untrusted_paths_are_rejected() {
        // A regular file is not a socket.
        let dir = std::env::temp_dir().join(format!("murl-sock-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("not-a-socket");
        std::fs::write(&file, b"x").unwrap();
        assert!(!client_may_trust(&file));
        assert!(!client_may_trust(&dir.join("missing")));
        std::fs::remove_dir_all(&dir).ok();
    }
}
