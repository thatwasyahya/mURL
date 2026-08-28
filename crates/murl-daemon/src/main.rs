//! `murl-daemon` — the resident mURL resolver.
//!
//! ```text
//! murl-daemon run [--socket PATH]     # serve until interrupted
//! murl-daemon status [--socket PATH]  # ask a running daemon how it's doing
//! murl-daemon path                    # print the socket path and exit
//! ```

use std::path::PathBuf;
use std::sync::atomic::AtomicU64;

use clap::{Parser, Subcommand};

use murl_core::cache::ManifestCache;
use murl_core::config::{HandlersFile, UserConfig};
use murl_core::error::Result;
use murl_core::fetch::LocalStore;
use murl_core::resolver::Resolver;
use murl_core::time::{Clock, SystemClock};
use murl_core::trust::TrustStore;

use murl_daemon::client;
use murl_daemon::protocol::{Request, Response, PROTOCOL_VERSION};
use murl_daemon::server::{self, Context};
use murl_daemon::socket;
use murl_daemon::terminal_ui::TerminalUi;

mod launcher;
mod paths;

#[derive(Parser)]
#[command(
    name = "murl-daemon",
    version,
    about = "Resident mURL resolver: a persistent consent surface for murl:// activations",
    long_about = "murl-daemon resolves mURLs and presents consent from a resident process,\n\
                  so activation does not depend on a terminal being available.\n\n\
                  It is never required: clients fall back to in-process resolution.\n\
                  Protocol and threat model: docs/daemon.md   Status: experimental"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Serve until interrupted
    Run {
        /// Socket path (default: $MURL_SOCKET, else $XDG_RUNTIME_DIR/murl/murl.sock)
        #[arg(long)]
        socket: Option<PathBuf>,
    },
    /// Query a running daemon
    Status {
        #[arg(long)]
        socket: Option<PathBuf>,
    },
    /// Print the socket path this daemon would use
    Path,
}

fn main() {
    let cli = Cli::parse();
    let code = match run(&cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("murl-daemon: error: {e}");
            1
        }
    };
    std::process::exit(code);
}

fn run(cli: &Cli) -> Result<i32> {
    match &cli.command {
        Command::Path => {
            println!("{}", socket::socket_path()?.display());
            Ok(0)
        }
        Command::Status { socket: path } => status(path.clone()),
        Command::Run { socket: path } => {
            let path = match path {
                Some(p) => p.clone(),
                None => socket::socket_path()?,
            };
            serve(&path)
        }
    }
}

fn status(path: Option<PathBuf>) -> Result<i32> {
    let path = match path {
        Some(p) => p,
        None => socket::socket_path()?,
    };
    match client::request(
        &path,
        &Request::Status {
            protocol: PROTOCOL_VERSION,
        },
    ) {
        Ok(responses) => {
            for response in responses {
                match response {
                    Response::Status {
                        version,
                        uptime_secs,
                        activations,
                        socket,
                    } => {
                        println!("running   {version}");
                        println!("socket    {socket}");
                        println!("uptime    {uptime_secs}s");
                        println!("served    {activations} activation(s)");
                    }
                    Response::Error { stage, message } => {
                        println!("error ({stage}): {message}");
                        return Ok(1);
                    }
                    other => println!("{other:?}"),
                }
            }
            Ok(0)
        }
        Err(e) => {
            println!("no usable daemon at {} ({e})", path.display());
            Ok(1)
        }
    }
}

fn serve(path: &std::path::Path) -> Result<i32> {
    // Stores and config are discovered exactly as the CLI discovers them,
    // so a daemon and a direct `murl open` see the same world.
    let dirs = paths::DaemonPaths::discover()?;
    let store = LocalStore::new(dirs.names_dir());
    let cache = ManifestCache::new(dirs.manifest_cache_dir());
    let trust = TrustStore::load(dirs.trust_file())?;
    // The user's policy, limits, and handlers — read through the same
    // loader the CLI uses. Carrying defaults here instead would mean a
    // configured `"dangerous": "deny"` became a clickable prompt and
    // configured handlers vanished, with both halves looking correct.
    let config = UserConfig::load(&dirs.config_file())?;
    let limits = config.limits();
    let policy = config.policy();
    let handlers = HandlersFile::load(&dirs.handlers_file())?;
    let opener = handlers.to_opener(std::env::consts::OS, dirs.home.clone());
    let clock = SystemClock;
    let launcher = launcher::RealLauncher;
    let consent = TerminalUi;
    let fetcher = murl_net::HttpsFetcher::with_trace(|message| {
        eprintln!("murl-daemon: {message}");
    });

    let resolver_limits = limits.clone();
    let with_resolver = |f: &mut dyn FnMut(&Resolver<'_>) -> Result<()>| -> Result<()> {
        let resolver = Resolver {
            local_store: &store,
            remote: Some(&fetcher),
            cache: Some(&cache),
            trust_store: &trust,
            limits: resolver_limits.clone(),
            clock: &clock,
        };
        f(&resolver)
    };

    let ctx = Context {
        with_resolver: &with_resolver,
        policy,
        opener,
        launcher: &launcher,
        consent: &consent,
        limits,
        started_at: clock.now_epoch(),
        socket: path.display().to_string(),
        activations: AtomicU64::new(0),
        version: env!("CARGO_PKG_VERSION"),
    };

    server::run(&ctx, path, &clock)?;
    Ok(0)
}
