//! The resource kind registry.
//!
//! Kinds are the extension point of the resource model: they name *what a
//! target is*, which determines how it is validated, classified, and
//! dispatched. The built-in kinds cover the MVP; `custom:<name>` kinds are
//! dispatched only through explicitly user-registered handlers and are always
//! classified DANGEROUS (see `docs/resource-types.md`).

use std::fmt;

use serde::{Deserialize, Serialize};

/// A parsed resource kind.
// Kinds grow in minor releases (docs/stability.md), so adding one must
// not be a breaking change for downstream matchers.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Kind {
    /// A web resource, dispatched to the default browser. Targets must be
    /// `https://` (plain `http://` is accepted for loopback hosts only).
    Https,
    /// A local file, dispatched to the platform opener.
    File,
    /// A local directory, dispatched to the file manager.
    Dir,
    /// A nested mURL. Never dispatched itself: it is resolved recursively and
    /// its resources are spliced into the plan, subject to depth/count limits.
    Murl,
    /// A terminal session rooted at a directory. Requires a user-configured
    /// terminal handler and is always DANGEROUS.
    Terminal,
    /// A remote shell session (`ssh://[user@]host[:port]`). Requires a
    /// user-configured handler and is always DANGEROUS: a remote shell is
    /// arbitrary code execution wherever it lands.
    Ssh,
    /// A remote desktop session (`rdp://host` / `vnc://host`). Requires a
    /// user-configured handler; DANGEROUS for the same reason as `ssh`.
    RemoteDesktop,
    /// A geographic location (`geo:lat,lon[;u=radius]`, RFC 5870). SAFE: it
    /// opens a map viewer and carries no capability.
    Geo,
    /// A pre-addressed message (`mailto:` per RFC 6068). SAFE: composing is
    /// not sending, and every mail client shows the draft first.
    Mailto,
    /// An extension kind (`custom:<name>`). Dispatched only via a handler the
    /// user registered out-of-band; unregistered custom kinds never launch.
    Custom(String),
}

impl Kind {
    pub const MAX_CUSTOM_NAME: usize = 32;

    /// Parse a kind string from a manifest.
    pub fn parse(s: &str) -> Result<Kind, String> {
        match s {
            "https" => Ok(Kind::Https),
            "file" => Ok(Kind::File),
            "dir" => Ok(Kind::Dir),
            "murl" => Ok(Kind::Murl),
            "terminal" => Ok(Kind::Terminal),
            "ssh" => Ok(Kind::Ssh),
            "remote-desktop" => Ok(Kind::RemoteDesktop),
            "geo" => Ok(Kind::Geo),
            "mailto" => Ok(Kind::Mailto),
            other => {
                if let Some(name) = other.strip_prefix("custom:") {
                    if name.is_empty() || name.len() > Self::MAX_CUSTOM_NAME {
                        return Err(format!("custom kind name `{name}` has invalid length"));
                    }
                    let ok = name.bytes().enumerate().all(|(i, b)| {
                        let alnum = b.is_ascii_lowercase() || b.is_ascii_digit();
                        if i == 0 {
                            alnum
                        } else {
                            alnum || b == b'-' || b == b'_'
                        }
                    });
                    if !ok {
                        return Err(format!(
                            "custom kind name `{name}` must match [a-z0-9][a-z0-9_-]*"
                        ));
                    }
                    Ok(Kind::Custom(name.to_owned()))
                } else {
                    Err(format!(
                        "unknown kind `{other}` (known: https, file, dir, murl, terminal, ssh, remote-desktop, geo, mailto, custom:<name>)"
                    ))
                }
            }
        }
    }

    /// True when this kind's target refers to the local filesystem.
    pub fn is_filesystem(&self) -> bool {
        matches!(self, Kind::File | Kind::Dir | Kind::Terminal)
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Kind::Https => f.write_str("https"),
            Kind::File => f.write_str("file"),
            Kind::Dir => f.write_str("dir"),
            Kind::Murl => f.write_str("murl"),
            Kind::Terminal => f.write_str("terminal"),
            Kind::Ssh => f.write_str("ssh"),
            Kind::RemoteDesktop => f.write_str("remote-desktop"),
            Kind::Geo => f.write_str("geo"),
            Kind::Mailto => f.write_str("mailto"),
            Kind::Custom(name) => write!(f, "custom:{name}"),
        }
    }
}

/// File extensions whose *opening* is code execution (or one dialog away from
/// it) on at least one supported platform. A `file` resource with one of
/// these extensions is escalated from SENSITIVE to DANGEROUS: `xdg-open` on a
/// `.desktop` file or `start` on an `.exe` runs it, it does not "view" it.
pub const EXECUTABLE_EXTENSIONS: &[&str] = &[
    "desktop", "exe", "bat", "cmd", "com", "scr", "msi", "ps1", "vbs", "vbe", "js", "jse", "wsf",
    "wsh", "hta", "sh", "run", "appimage", "jar", "lnk", "url", "reg", "app",
];

/// Case-insensitive executable-extension check on a target path.
///
/// Trailing dots and spaces are stripped before the extension is read.
/// Windows discards them when resolving a filename, so `setup.exe.` and
/// `setup.exe ` both open `setup.exe` — reading the extension literally
/// would classify those as SENSITIVE and hand an executable the
/// consent path meant for a document.
pub fn has_executable_extension(target: &str) -> bool {
    let lower = target.to_ascii_lowercase();
    let name = lower.rsplit(['/', '\\']).next().unwrap_or(&lower);
    let name = name.trim_end_matches(['.', ' ']);
    match name.rsplit_once('.') {
        Some((_, ext)) => EXECUTABLE_EXTENSIONS.contains(&ext),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_builtin_kinds() {
        assert_eq!(Kind::parse("https").unwrap(), Kind::Https);
        assert_eq!(Kind::parse("terminal").unwrap(), Kind::Terminal);
        assert_eq!(Kind::parse("ssh").unwrap(), Kind::Ssh);
        assert_eq!(Kind::parse("remote-desktop").unwrap(), Kind::RemoteDesktop);
        assert_eq!(Kind::parse("geo").unwrap(), Kind::Geo);
        assert_eq!(Kind::parse("mailto").unwrap(), Kind::Mailto);
        assert_eq!(
            Kind::parse("custom:vscode").unwrap(),
            Kind::Custom("vscode".into())
        );
    }

    #[test]
    fn rejects_bad_kinds() {
        assert!(Kind::parse("http").is_err());
        assert!(Kind::parse("HTTPS").is_err());
        assert!(Kind::parse("custom:").is_err());
        assert!(Kind::parse("custom:UPPER").is_err());
        assert!(Kind::parse("custom:-x").is_err());
        assert!(Kind::parse("exec").is_err());
        assert!(Kind::parse("").is_err());
    }

    #[test]
    fn roundtrips_display() {
        for s in [
            "https",
            "file",
            "dir",
            "murl",
            "terminal",
            "ssh",
            "remote-desktop",
            "geo",
            "mailto",
            "custom:my-app",
        ] {
            assert_eq!(Kind::parse(s).unwrap().to_string(), s);
        }
    }

    #[test]
    fn executable_extension_detection() {
        assert!(has_executable_extension("/home/u/installer.sh"));
        assert!(has_executable_extension("C:\\tools\\setup.EXE"));
        assert!(has_executable_extension("/home/u/app.desktop"));
        assert!(has_executable_extension("/home/u/evil.tar.gz.bat"));
        // Windows drops trailing dots and spaces when opening a file, so
        // these all reach the same executable.
        assert!(has_executable_extension("C:\\tools\\setup.exe."));
        assert!(has_executable_extension("C:\\tools\\setup.exe "));
        assert!(has_executable_extension("C:\\tools\\setup.exe. . "));
        assert!(!has_executable_extension("/home/u/report.pdf."));
        assert!(!has_executable_extension("/home/u/report.pdf"));
        assert!(!has_executable_extension("/home/u/README"));
        assert!(!has_executable_extension("/home/u/archive.tar.gz"));
    }
}
