//! The real process launcher: argv in, detached child out. No shell is ever
//! involved — `Command::new(argv[0]).args(&argv[1..])` goes straight to the
//! OS process API, so a hostile target can never become shell syntax.

use std::path::Path;
use std::process::{Command, Stdio};

use murl_core::dispatch::Launcher;
use murl_core::error::{Error, Result};

use crate::logger;

#[derive(Debug, Default)]
pub struct RealLauncher;

impl Launcher for RealLauncher {
    fn launch(&self, argv: &[String], cwd: Option<&Path>) -> Result<()> {
        let (program, args) = argv
            .split_first()
            .ok_or_else(|| Error::Dispatch("empty argv".into()))?;
        logger::debug(&format!("launching {argv:?} (cwd: {cwd:?})"));
        let mut cmd = Command::new(program);
        push_args(&mut cmd, program, args)?;
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
        cmd.spawn()
            .map(drop)
            .map_err(|e| Error::Dispatch(format!("failed to launch `{program}`: {e}")))
    }

    fn path_exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn sleep_ms(&self, ms: u64) {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
}

/// Explorer splits its command line at commas and obeys switches it finds
/// there, so on Windows each argument reaches it as one quoted token (see
/// [`murl_core::dispatch::explorer_argument`]). Every other program gets
/// the standard quoting.
#[cfg(windows)]
fn push_args(cmd: &mut Command, program: &str, args: &[String]) -> Result<()> {
    use murl_core::dispatch::{explorer_argument, is_explorer};
    use std::os::windows::process::CommandExt;
    if !is_explorer(program) {
        cmd.args(args);
        return Ok(());
    }
    for arg in args {
        cmd.raw_arg(explorer_argument(arg).map_err(Error::Dispatch)?);
    }
    Ok(())
}

#[cfg(not(windows))]
fn push_args(cmd: &mut Command, _program: &str, args: &[String]) -> Result<()> {
    cmd.args(args);
    Ok(())
}
