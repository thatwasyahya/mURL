//! Process launching for the daemon: argv arrays straight to the OS, the
//! same rule as the CLI's launcher. No shell is ever involved, so a hostile
//! target cannot become shell syntax.

use std::path::Path;
use std::process::{Command, Stdio};

use murl_core::dispatch::Launcher;
use murl_core::error::{Error, Result};

#[derive(Debug, Default)]
pub struct RealLauncher;

impl Launcher for RealLauncher {
    fn launch(&self, argv: &[String], cwd: Option<&Path>) -> Result<()> {
        let (program, args) = argv
            .split_first()
            .ok_or_else(|| Error::Dispatch("empty argv".into()))?;
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
