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
        cmd.args(args)
            .stdin(Stdio::null())
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
