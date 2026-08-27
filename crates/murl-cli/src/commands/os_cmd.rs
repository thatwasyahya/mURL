//! `murl os` — register/unregister the `murl://` scheme with the OS.
//!
//! Linux (implemented): a desktop entry claiming `x-scheme-handler/murl`,
//! activated via `xdg-mime`. Windows (implemented): `HKCU\Software\Classes\
//! murl` via `reg.exe` — per-user, no elevation. macOS (documented stub):
//! Launch Services only reads URL schemes from an application bundle's
//! Info.plist, so a bare CLI binary cannot self-register; see
//! `docs/os-integration.md` for the packaging plan.
//!
//! Every external invocation is an argv array — no shell anywhere.

use std::path::PathBuf;
use std::process::Command;

use murl_core::error::{Error, Result};

use crate::ctx::App;
use crate::logger;

const DESKTOP_FILE: &str = "murl-handler.desktop";
const REG_KEY: &str = r"HKCU\Software\Classes\murl";

pub fn install(app: &App) -> Result<i32> {
    match std::env::consts::OS {
        "linux" => linux_install(app),
        "windows" => windows_install(),
        "macos" => {
            println!("macOS registration requires an application bundle (Launch Services reads");
            println!("CFBundleURLTypes from Info.plist; a bare binary cannot self-register).");
            println!("See docs/os-integration.md for the packaging plan.");
            Ok(1)
        }
        other => Err(Error::Dispatch(format!("unsupported platform `{other}`"))),
    }
}

pub fn uninstall(_app: &App) -> Result<i32> {
    match std::env::consts::OS {
        "linux" => linux_uninstall(),
        "windows" => windows_uninstall(),
        other => Err(Error::Dispatch(format!("unsupported platform `{other}`"))),
    }
}

pub fn status(_app: &App) -> Result<i32> {
    match std::env::consts::OS {
        "linux" => linux_status(),
        "windows" => windows_status(),
        other => {
            println!("no OS integration on `{other}` yet");
            Ok(0)
        }
    }
}

fn applications_dir() -> Result<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| crate::paths::home_dir().map(|h| h.join(".local/share")))
        .ok_or_else(|| Error::Dispatch("cannot determine XDG data directory".into()))?;
    Ok(base.join("applications"))
}

fn linux_install(_app: &App) -> Result<i32> {
    let exe = std::env::current_exe()?;
    let exe_str = exe.to_str().ok_or_else(|| {
        Error::Dispatch("executable path is not valid UTF-8; cannot write a desktop entry".into())
    })?;
    if exe_str.contains('"') || exe_str.contains('\n') {
        return Err(Error::Dispatch(
            "executable path contains characters unsafe for a desktop entry".into(),
        ));
    }

    let apps = applications_dir()?;
    std::fs::create_dir_all(&apps)?;
    let desktop_path = apps.join(DESKTOP_FILE);
    // Terminal=true so the consent prompt has a TTY. A native consent dialog
    // is the v0.3 daemon's job (docs/roadmap.md).
    let content = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=mURL Resolver\n\
         Comment=Resolve and open mURL multi-resource identifiers\n\
         Exec=\"{exe_str}\" open %u\n\
         Terminal=true\n\
         NoDisplay=true\n\
         MimeType=x-scheme-handler/murl;\n"
    );
    std::fs::write(&desktop_path, content)?;
    println!("wrote {}", desktop_path.display());

    run_quiet(
        "xdg-mime",
        &["default", DESKTOP_FILE, "x-scheme-handler/murl"],
    )?;
    println!("registered as the default handler for x-scheme-handler/murl");

    // Best-effort refresh; absence of the tool is not an error.
    if run_quiet(
        "update-desktop-database",
        &[apps.to_string_lossy().as_ref()],
    )
    .is_err()
    {
        logger::debug("update-desktop-database unavailable; desktop cache not refreshed");
    }

    println!();
    println!("try it: xdg-open murl://local/<name>");
    println!("note: activation opens a terminal for the consent prompt (Terminal=true).");
    Ok(0)
}

fn linux_uninstall() -> Result<i32> {
    let desktop_path = applications_dir()?.join(DESKTOP_FILE);
    if desktop_path.exists() {
        std::fs::remove_file(&desktop_path)?;
        println!("removed {}", desktop_path.display());
    } else {
        println!("{} was not installed", desktop_path.display());
    }
    println!("note: xdg-mime associations pointing at the removed entry become inert.");
    Ok(0)
}

fn linux_status() -> Result<i32> {
    let desktop_path = applications_dir()?.join(DESKTOP_FILE);
    println!(
        "desktop entry: {} ({})",
        desktop_path.display(),
        if desktop_path.exists() {
            "present"
        } else {
            "absent"
        }
    );
    match Command::new("xdg-mime")
        .args(["query", "default", "x-scheme-handler/murl"])
        .output()
    {
        Ok(out) => {
            let current = String::from_utf8_lossy(&out.stdout).trim().to_string();
            println!(
                "x-scheme-handler/murl -> {}",
                if current.is_empty() {
                    "(none)"
                } else {
                    &current
                }
            );
        }
        Err(e) => println!("xdg-mime unavailable: {e}"),
    }
    Ok(0)
}

fn windows_install() -> Result<i32> {
    let exe = std::env::current_exe()?;
    let exe_str = exe.to_string_lossy().into_owned();
    let command = format!("\"{exe_str}\" open \"%1\"");
    run_quiet(
        "reg.exe",
        &[
            "add",
            REG_KEY,
            "/ve",
            "/t",
            "REG_SZ",
            "/d",
            "URL:mURL Protocol",
            "/f",
        ],
    )?;
    run_quiet(
        "reg.exe",
        &[
            "add",
            REG_KEY,
            "/v",
            "URL Protocol",
            "/t",
            "REG_SZ",
            "/d",
            "",
            "/f",
        ],
    )?;
    let cmd_key = format!(r"{REG_KEY}\shell\open\command");
    run_quiet(
        "reg.exe",
        &["add", &cmd_key, "/ve", "/t", "REG_SZ", "/d", &command, "/f"],
    )?;
    println!("registered murl:// for the current user (HKCU)");
    Ok(0)
}

fn windows_uninstall() -> Result<i32> {
    run_quiet("reg.exe", &["delete", REG_KEY, "/f"])?;
    println!("unregistered murl:// for the current user");
    Ok(0)
}

fn windows_status() -> Result<i32> {
    let cmd_key = format!(r"{REG_KEY}\shell\open\command");
    match Command::new("reg.exe")
        .args(["query", &cmd_key, "/ve"])
        .output()
    {
        Ok(out) if out.status.success() => {
            println!("{}", String::from_utf8_lossy(&out.stdout).trim());
        }
        _ => println!("murl:// is not registered for the current user"),
    }
    Ok(0)
}

fn run_quiet(program: &str, args: &[&str]) -> Result<()> {
    let out = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| Error::Dispatch(format!("failed to run {program}: {e}")))?;
    if !out.status.success() {
        return Err(Error::Dispatch(format!(
            "{program} {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(())
}
