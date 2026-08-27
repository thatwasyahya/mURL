//! Minimal leveled logging to stderr.
//!
//! Deliberately not a logging framework: the CLI needs four levels, an env
//! override, and zero dependencies. Structured (JSON) *output* is a property
//! of command results (`--json` on stdout), not of the log stream.

use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
}

static LEVEL: OnceLock<Level> = OnceLock::new();

/// Initialize from `-v` count; `MURL_LOG=debug|info|warn|error` overrides.
pub fn init(verbosity: u8) {
    let from_flag = match verbosity {
        0 => Level::Warn,
        1 => Level::Info,
        _ => Level::Debug,
    };
    let level = match std::env::var("MURL_LOG").ok().as_deref() {
        Some("debug") => Level::Debug,
        Some("info") => Level::Info,
        Some("warn") => Level::Warn,
        Some("error") => Level::Error,
        _ => from_flag,
    };
    let _ = LEVEL.set(level);
}

fn enabled(level: Level) -> bool {
    level <= *LEVEL.get_or_init(|| Level::Warn)
}

pub fn error(msg: &str) {
    if enabled(Level::Error) {
        eprintln!("murl: error: {msg}");
    }
}

pub fn warn(msg: &str) {
    if enabled(Level::Warn) {
        eprintln!("murl: warning: {msg}");
    }
}

pub fn info(msg: &str) {
    if enabled(Level::Info) {
        eprintln!("murl: {msg}");
    }
}

pub fn debug(msg: &str) {
    if enabled(Level::Debug) {
        eprintln!("murl: debug: {msg}");
    }
}
