//! `murl handler` — configure how kinds map to local programs.
//!
//! Handler registration is a *local, explicit* act by the user; manifests
//! can never register or alter handlers. That asymmetry is the point: a
//! manifest names what it wants opened, only the user decides what programs
//! that can possibly mean.

use murl_core::error::{Error, Result};
use murl_core::kind::Kind;

use crate::ctx::{load_handlers, save_handlers, App};

fn validate_argv(argv: &[String]) -> Result<()> {
    if argv.is_empty() || argv[0].trim().is_empty() {
        return Err(Error::Validation(
            "handler argv must start with a program".into(),
        ));
    }
    Ok(())
}

/// `murl handler set-ssh <argv...>` — the client for `ssh` resources.
pub fn set_ssh(app: &App, argv: &[String]) -> Result<i32> {
    validate_argv(argv)?;
    let path = app.paths.handlers_file();
    let mut handlers = load_handlers(&path)?;
    handlers.ssh = Some(argv.to_vec());
    save_handlers(&path, &handlers)?;
    println!("ssh handler set: {argv:?}");
    note_target_placeholder(argv, "the ssh:// URL");
    Ok(0)
}

/// `murl handler set-remote-desktop <argv...>` — the client for
/// `remote-desktop` resources.
pub fn set_remote_desktop(app: &App, argv: &[String]) -> Result<i32> {
    validate_argv(argv)?;
    let path = app.paths.handlers_file();
    let mut handlers = load_handlers(&path)?;
    handlers.remote_desktop = Some(argv.to_vec());
    save_handlers(&path, &handlers)?;
    println!("remote-desktop handler set: {argv:?}");
    note_target_placeholder(argv, "the rdp:// or vnc:// URL");
    Ok(0)
}

fn note_target_placeholder(argv: &[String], what: &str) {
    if !argv.iter().any(|a| a.contains("{target}")) {
        println!(
            "note: no element contains {{target}}; {what} will be appended as the last argument"
        );
    }
}

pub fn set_terminal(app: &App, argv: &[String]) -> Result<i32> {
    validate_argv(argv)?;
    let path = app.paths.handlers_file();
    let mut handlers = load_handlers(&path)?;
    handlers.terminal = Some(argv.to_vec());
    save_handlers(&path, &handlers)?;
    println!("terminal handler set: {argv:?}");
    if !argv.iter().any(|a| a.contains("{target}")) {
        println!("note: no element contains {{target}}; the directory will be appended as the last argument");
    }
    Ok(0)
}

pub fn register(app: &App, kind: &str, argv: &[String]) -> Result<i32> {
    validate_argv(argv)?;
    let name = kind.strip_prefix("custom:").unwrap_or(kind);
    // Reuse the kind grammar so registered names always match manifests.
    Kind::parse(&format!("custom:{name}")).map_err(Error::Validation)?;
    let path = app.paths.handlers_file();
    let mut handlers = load_handlers(&path)?;
    handlers.custom.insert(name.to_owned(), argv.to_vec());
    save_handlers(&path, &handlers)?;
    println!("registered handler for custom:{name}: {argv:?}");
    Ok(0)
}

pub fn list(app: &App) -> Result<i32> {
    let handlers = load_handlers(&app.paths.handlers_file())?;
    if app.json {
        println!("{}", serde_json::to_string_pretty(&handlers)?);
        return Ok(0);
    }
    println!(
        "open:     {:?}  (platform default when unset)",
        handlers.open.unwrap_or_default()
    );
    match &handlers.terminal {
        Some(argv) => println!("terminal: {argv:?}"),
        None => println!("terminal: (unset — terminal resources cannot dispatch)"),
    }
    match &handlers.ssh {
        Some(argv) => println!("ssh:      {argv:?}"),
        None => println!("ssh:      (unset — ssh resources cannot dispatch)"),
    }
    match &handlers.remote_desktop {
        Some(argv) => println!("rdesktop: {argv:?}"),
        None => println!("rdesktop: (unset — remote-desktop resources cannot dispatch)"),
    }
    if handlers.custom.is_empty() {
        println!("custom:   (none)");
    } else {
        for (name, argv) in &handlers.custom {
            println!("custom:{name}: {argv:?}");
        }
    }
    Ok(0)
}

pub fn remove(app: &App, kind: &str) -> Result<i32> {
    let name = kind.strip_prefix("custom:").unwrap_or(kind);
    let path = app.paths.handlers_file();
    let mut handlers = load_handlers(&path)?;
    if handlers.custom.remove(name).is_some() {
        save_handlers(&path, &handlers)?;
        println!("removed handler for custom:{name}");
        Ok(0)
    } else {
        Err(Error::NotFound(format!(
            "no handler registered for custom:{name}"
        )))
    }
}
