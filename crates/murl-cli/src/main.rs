//! `murl` — the reference CLI for mURL (Multi-Resource Uniform Locator).
//!
//! Exit codes (stable, scripts may rely on them):
//!
//! * `0` — success
//! * `1` — error (parse, resolution, I/O, dispatch failure)
//! * `2` — validation failed / signature missing where one was requested
//! * `3` — partial success (`open`: some resources did not open)
//! * `4` — denied (`open`: nothing was approved, or policy refused)

mod commands;
mod consent;
mod ctx;
mod httpfetch;
mod launcher;
mod logger;
mod paths;
mod render;

use clap::{Parser, Subcommand};

use murl_core::Error;

#[derive(Parser)]
#[command(
    name = "murl",
    version,
    about = "mURL: one identifier that opens a whole working context",
    long_about = "mURL (Multi-Resource Uniform Locator) resolves one stable identifier\n\
                  (murl://authority/name) into a manifest describing a set of resources —\n\
                  web URLs, local files, directories, terminals, nested mURLs — and\n\
                  dispatches each to its handler, under a consent-based security policy.\n\n\
                  Specification: spec/SPECIFICATION.md   Status: experimental (v0.1)"
)]
struct Cli {
    /// Emit machine-readable JSON on stdout
    #[arg(long, global = true)]
    json: bool,

    /// Never touch the network (local store and cache only)
    #[arg(long, global = true)]
    offline: bool,

    /// Ignore cached manifests for this invocation
    #[arg(long, global = true)]
    refresh: bool,

    /// Increase log verbosity (-v info, -vv debug); logs go to stderr
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse an mURL and print its components
    Parse {
        /// The mURL string, e.g. murl://example.com/team/project-x@1.2.0#docs
        murl: String,
    },
    /// Create a new manifest from a template
    Create {
        /// Destination name shown to users, e.g. "Project X"
        #[arg(long)]
        name: Option<String>,
        /// Output file (default: derived from the name; `-` for stdout)
        #[arg(short, long)]
        output: Option<String>,
        /// Overwrite an existing file
        #[arg(long)]
        force: bool,
    },
    /// Validate a manifest (file path or mURL) against the specification
    Validate {
        /// A manifest file path or an mURL
        target: String,
    },
    /// Summarize a manifest: metadata, resources, tiers, trust
    Inspect {
        /// A manifest file path or an mURL
        target: String,
    },
    /// Resolve an mURL into its full dispatch plan without opening anything
    Resolve {
        /// A manifest file path or an mURL
        target: String,
    },
    /// Resolve an mURL and open its resources (with consent)
    Open {
        /// A manifest file path or an mURL
        target: String,
        /// Show the plan and exit without launching anything
        #[arg(long)]
        dry_run: bool,
        /// Approve SAFE resources without prompting
        #[arg(short = 'y', long)]
        yes: bool,
        /// Approve SENSITIVE resources (local files/directories)
        #[arg(long)]
        allow_sensitive: bool,
        /// Approve DANGEROUS resources (terminals, executables, custom
        /// handlers). Still subject to the trust requirement.
        #[arg(long)]
        allow_dangerous: bool,
        /// Open only these resource ids (repeatable)
        #[arg(long)]
        only: Vec<String>,
        /// Skip these resource ids (repeatable)
        #[arg(long)]
        skip: Vec<String>,
    },
    /// Manage the local name store (murl://local/...)
    #[command(subcommand)]
    Name(NameCmd),
    /// Generate an ed25519 signing keypair
    Keygen {
        /// Key file path (default: <config>/keys/default.key.json)
        #[arg(long)]
        out: Option<String>,
        /// Overwrite an existing key file
        #[arg(long)]
        force: bool,
    },
    /// Sign a manifest file in place
    Sign {
        /// Manifest file to sign
        file: String,
        /// Key file (default: <config>/keys/default.key.json)
        #[arg(long)]
        key: Option<String>,
    },
    /// Verify a manifest's signature and report its trust status
    Verify {
        /// A manifest file path or an mURL
        target: String,
    },
    /// Manage trusted signing keys, pinned per authority
    #[command(subcommand)]
    Trust(TrustCmd),
    /// Manage the remote-manifest cache
    #[command(subcommand)]
    Cache(CacheCmd),
    /// Register or unregister murl:// with the operating system
    #[command(subcommand)]
    Os(OsCmd),
    /// Configure resource handlers (terminal, custom kinds)
    #[command(subcommand)]
    Handler(HandlerCmd),
}

#[derive(Subcommand)]
enum NameCmd {
    /// Install a manifest under a local name (validates first)
    Add {
        /// The local name, e.g. `project-x` or `team/project-x@1.0.0`
        name: String,
        /// Manifest file to install
        file: String,
    },
    /// List installed local names
    List,
    /// Remove a local name
    Remove { name: String },
}

#[derive(Subcommand)]
enum TrustCmd {
    /// Pin a signing key for an authority
    Add {
        /// The authority, e.g. `example.com`
        authority: String,
        /// Base64 public key, or a path to a key/manifest file containing one
        key: String,
    },
    /// List pinned keys
    List,
    /// Remove a pinned key
    Remove { authority: String, key_id: String },
}

#[derive(Subcommand)]
enum CacheCmd {
    /// List cached manifests
    List,
    /// Remove one cached manifest
    Evict { murl: String },
    /// Remove all cached manifests
    Clear,
}

#[derive(Subcommand)]
enum OsCmd {
    /// Register the murl:// scheme handler for the current user
    Install,
    /// Unregister the murl:// scheme handler
    Uninstall,
    /// Show the current registration state
    Status,
}

#[derive(Subcommand)]
enum HandlerCmd {
    /// Set the terminal handler argv; use {target} for the directory.
    /// Example: murl handler set-terminal gnome-terminal --working-directory={target}
    SetTerminal {
        #[arg(trailing_var_arg = true, required = true)]
        argv: Vec<String>,
    },
    /// Register a handler for a custom kind.
    /// Example: murl handler register vscode -- code --folder-uri {target}
    Register {
        /// Custom kind name (without the `custom:` prefix)
        kind: String,
        #[arg(trailing_var_arg = true, required = true)]
        argv: Vec<String>,
    },
    /// List configured handlers
    List,
    /// Remove a custom-kind handler
    Remove { kind: String },
}

fn main() {
    let cli = Cli::parse();
    logger::init(cli.verbose);

    let result = run(&cli);
    let code = match result {
        Ok(code) => code,
        Err(e) => {
            logger::error(&e.to_string());
            match e {
                Error::Validation(_) => 2,
                Error::Denied(_) => 4,
                _ => 1,
            }
        }
    };
    std::process::exit(code);
}

fn run(cli: &Cli) -> murl_core::Result<i32> {
    let app = ctx::App::init(cli.json, cli.offline, cli.refresh)?;
    match &cli.command {
        Command::Parse { murl } => commands::parse_cmd::run(&app, murl),
        Command::Create {
            name,
            output,
            force,
        } => commands::create::run(&app, name.as_deref(), output.as_deref(), *force),
        Command::Validate { target } => commands::validate::run(&app, target),
        Command::Inspect { target } => commands::inspect::run(&app, target),
        Command::Resolve { target } => commands::resolve::run(&app, target),
        Command::Open {
            target,
            dry_run,
            yes,
            allow_sensitive,
            allow_dangerous,
            only,
            skip,
        } => commands::open::run(
            &app,
            target,
            commands::open::OpenOptions {
                dry_run: *dry_run,
                yes: *yes,
                allow_sensitive: *allow_sensitive,
                allow_dangerous: *allow_dangerous,
                only: only.clone(),
                skip: skip.clone(),
            },
        ),
        Command::Name(sub) => match sub {
            NameCmd::Add { name, file } => commands::name::add(&app, name, file),
            NameCmd::List => commands::name::list(&app),
            NameCmd::Remove { name } => commands::name::remove(&app, name),
        },
        Command::Keygen { out, force } => commands::keys::keygen(&app, out.as_deref(), *force),
        Command::Sign { file, key } => commands::keys::sign(&app, file, key.as_deref()),
        Command::Verify { target } => commands::keys::verify(&app, target),
        Command::Trust(sub) => match sub {
            TrustCmd::Add { authority, key } => commands::trust_cmd::add(&app, authority, key),
            TrustCmd::List => commands::trust_cmd::list(&app),
            TrustCmd::Remove { authority, key_id } => {
                commands::trust_cmd::remove(&app, authority, key_id)
            }
        },
        Command::Cache(sub) => match sub {
            CacheCmd::List => commands::cache_cmd::list(&app),
            CacheCmd::Evict { murl } => commands::cache_cmd::evict(&app, murl),
            CacheCmd::Clear => commands::cache_cmd::clear(&app),
        },
        Command::Os(sub) => match sub {
            OsCmd::Install => commands::os_cmd::install(&app),
            OsCmd::Uninstall => commands::os_cmd::uninstall(&app),
            OsCmd::Status => commands::os_cmd::status(&app),
        },
        Command::Handler(sub) => match sub {
            HandlerCmd::SetTerminal { argv } => commands::handler_cmd::set_terminal(&app, argv),
            HandlerCmd::Register { kind, argv } => {
                commands::handler_cmd::register(&app, kind, argv)
            }
            HandlerCmd::List => commands::handler_cmd::list(&app),
            HandlerCmd::Remove { kind } => commands::handler_cmd::remove(&app, kind),
        },
    }
}
