//! The dispatch engine: turning an approved plan into launched handlers.
//!
//! Two hard rules, enforced structurally:
//!
//! 1. **No shell, ever.** Every launch is an argv array handed to the
//!    process API. Targets are data; there is no string that gets
//!    re-interpreted by `sh -c` or `cmd /c`.
//! 2. **Core plans, embedder launches.** This module builds argv vectors and
//!    sequences outcomes, but actual process creation goes through the
//!    [`Launcher`] trait — the CLI provides the real one, tests provide a
//!    recorder. Dispatch logic is therefore fully testable without opening
//!    forty browser tabs.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::{Error, Result};
use crate::kind::Kind;
use crate::limits::Limits;
use crate::policy::Tier;
use crate::resolver::Resolution;

/// Process launching + the few effects dispatch needs, as a trait.
pub trait Launcher: std::fmt::Debug {
    /// Spawn `argv` (argv[0] = program) detached, optionally with a working
    /// directory. Must not block on the child.
    fn launch(&self, argv: &[String], cwd: Option<&Path>) -> Result<()>;
    fn path_exists(&self, path: &Path) -> bool;
    fn sleep_ms(&self, ms: u64);
}

/// Platform opener configuration: how each kind maps to argv.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct OpenerConfig {
    /// The generic opener for URLs, files, and directories,
    /// e.g. `["xdg-open"]`, `["open"]`, `["explorer.exe"]`.
    pub open_argv: Vec<String>,
    /// Handler for `terminal` resources. `{target}` is substituted with the
    /// working directory. Unset means terminal resources cannot dispatch.
    pub terminal_argv: Option<Vec<String>>,
    /// Handler for `ssh` resources. `{target}` is the full `ssh://…` URL.
    /// Unset means ssh resources cannot dispatch — no guessing.
    pub ssh_argv: Option<Vec<String>>,
    /// Handler for `remote-desktop` resources (`rdp://` / `vnc://`).
    pub remote_desktop_argv: Option<Vec<String>>,
    /// Handlers for `custom:<name>` kinds, keyed by name (without prefix).
    pub custom: BTreeMap<String, Vec<String>>,
    /// Home directory used for `~` expansion in path targets.
    pub home_dir: Option<PathBuf>,
}

impl OpenerConfig {
    /// Defaults for a platform (`std::env::consts::OS` values).
    pub fn platform_default(os: &str, home_dir: Option<PathBuf>) -> OpenerConfig {
        let open_argv = match os {
            "macos" => vec!["open".to_string()],
            // `explorer.exe` accepts URLs, files, and directories without
            // involving cmd.exe (`start` is a shell builtin — an injection
            // surface this project refuses to have).
            "windows" => vec!["explorer.exe".to_string()],
            _ => vec!["xdg-open".to_string()],
        };
        OpenerConfig {
            open_argv,
            terminal_argv: None,
            ssh_argv: None,
            remote_desktop_argv: None,
            custom: BTreeMap::new(),
            home_dir,
        }
    }

    /// Expand `~`/`~/x` against the configured home directory.
    ///
    /// `Path::join` *replaces* the base when given an absolute path, so a
    /// naive `home.join(&target[2..])` turns `~//etc/passwd` into
    /// `/etc/passwd` — the consent prompt would show a path under the home
    /// directory while something else entirely was opened. The remainder is
    /// therefore joined segment by segment, and an empty or rooted
    /// remainder is refused outright. `push` replaces the base for a
    /// segment with a Windows drive prefix too (`Z:\x`, `Z:x`), so only a
    /// segment made of plain names is pushed.
    fn expand_path(&self, target: &str) -> std::result::Result<PathBuf, String> {
        let Some(rest) = target.strip_prefix('~') else {
            return Ok(PathBuf::from(target));
        };
        let home = self
            .home_dir
            .as_ref()
            .ok_or_else(|| "no home directory available for ~ expansion".to_string())?;
        if rest.is_empty() {
            return Ok(home.clone());
        }
        let Some(rest) = rest.strip_prefix('/') else {
            // `~user/...` is not a form mURL defines.
            return Err(format!("`{target}` is not a `~` or `~/…` path"));
        };
        // One trailing separator (`~/projects/`) is allowed, as in the
        // validator; it adds no segment.
        let rest = rest.strip_suffix('/').unwrap_or(rest);
        let mut path = home.clone();
        if rest.is_empty() {
            return Ok(path);
        }
        for segment in rest.split('/') {
            if segment.is_empty() {
                return Err(format!(
                    "`{target}` has an empty path segment; it would escape the home directory"
                ));
            }
            if segment == "." || segment == ".." {
                return Err(format!("`{target}` contains a dot segment"));
            }
            let plain = Path::new(segment)
                .components()
                .all(|c| matches!(c, Component::Normal(_)));
            if !plain {
                return Err(format!(
                    "`{target}` has a segment that is not a plain name; it would escape the home directory"
                ));
            }
            path.push(segment);
        }
        Ok(path)
    }
}

/// True when `program` is Windows Explorer, however it is spelled in a
/// handler (`explorer.exe`, `explorer`, a full path).
pub fn is_explorer(program: &str) -> bool {
    let name = program.rsplit(['/', '\\']).next().unwrap_or(program);
    name.eq_ignore_ascii_case("explorer.exe") || name.eq_ignore_ascii_case("explorer")
}

/// The command-line text that hands `arg` to explorer.exe as one argument.
///
/// Explorer does not split its command line the way other programs do. It
/// splits it at commas and acts on its own switches wherever they appear,
/// so given `https://a.example/x,/root,\\host\share` it opens the share,
/// not the browser: a target that looks like a web link reaches a network
/// path instead, connecting to the host it names. A quoted argument is taken
/// whole, commas included, and the URL reaches the browser unchanged.
///
/// Explorer's quotes simply toggle (`"C:\dir\"` opens `C:\dir\`; there is
/// no backslash escaping), so the argument is wrapped as it is, and one
/// containing a quote, which could close the quoting early, is refused. A
/// quote is legal in neither a URI nor a Windows path.
pub fn explorer_argument(arg: &str) -> std::result::Result<String, String> {
    if arg.contains('"') {
        return Err(format!(
            "`{arg}` contains a double quote, which explorer.exe cannot be given safely"
        ));
    }
    Ok(format!("\"{arg}\""))
}

/// Per-resource consent verdict, parallel to `Resolution::resources`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Approval {
    Approved,
    Denied(String),
    Skipped(String),
}

/// What happened to one resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum OutcomeStatus {
    Opened,
    Skipped,
    Denied,
    Failed,
    Unavailable,
}

impl std::fmt::Display for OutcomeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            OutcomeStatus::Opened => "OPENED",
            OutcomeStatus::Skipped => "SKIPPED",
            OutcomeStatus::Denied => "DENIED",
            OutcomeStatus::Failed => "FAILED",
            OutcomeStatus::Unavailable => "UNAVAILABLE",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceOutcome {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub tier: Tier,
    pub status: OutcomeStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Aggregate result of an activation (spec §9 failure semantics).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AggregateStatus {
    /// Every non-skipped resource opened.
    Success,
    /// Some opened, some did not; no required resource failed.
    ///
    /// The wire name is `PARTIAL_SUCCESS`, matching the specification and
    /// the `Display` impl. They disagreed once — serde derived `PARTIAL`
    /// from the variant name — and a consumer that matched on the
    /// documented string silently fell through to its error branch.
    #[serde(rename = "PARTIAL_SUCCESS")]
    Partial,
    /// A required resource did not open, or nothing opened and something failed.
    Failed,
    /// Nothing opened because everything was denied.
    Denied,
}

impl std::fmt::Display for AggregateStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            AggregateStatus::Success => "SUCCESS",
            AggregateStatus::Partial => "PARTIAL_SUCCESS",
            AggregateStatus::Failed => "FAILED",
            AggregateStatus::Denied => "DENIED",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Clone)]
pub struct ExecutionReport {
    pub outcomes: Vec<ResourceOutcome>,
    pub aggregate: AggregateStatus,
}

impl ExecutionReport {
    pub fn to_json(&self) -> serde_json::Value {
        json!({
            "aggregate": self.aggregate,
            "outcomes": self.outcomes,
        })
    }
}

/// Execute the approved plan. Launches are sequential in plan order with a
/// stagger delay — deliberately not parallel: a desktop absorbing N app
/// launches at once is a worse experience *and* a resource-exhaustion
/// surface.
pub fn execute(
    resolution: &Resolution,
    approvals: &[Approval],
    opener: &OpenerConfig,
    launcher: &dyn Launcher,
    limits: &Limits,
) -> Result<ExecutionReport> {
    if approvals.len() != resolution.resources.len() {
        return Err(Error::Dispatch(format!(
            "approvals ({}) do not match planned resources ({})",
            approvals.len(),
            resolution.resources.len()
        )));
    }

    let mut outcomes = Vec::with_capacity(approvals.len());
    let mut launched_any = false;

    for (pr, approval) in resolution.resources.iter().zip(approvals) {
        let mut outcome = ResourceOutcome {
            id: pr.resource.id.clone(),
            label: pr.display_label().to_owned(),
            kind: pr.kind.to_string(),
            tier: pr.tier,
            status: OutcomeStatus::Skipped,
            detail: None,
        };

        match approval {
            Approval::Denied(reason) => {
                outcome.status = OutcomeStatus::Denied;
                outcome.detail = Some(reason.clone());
            }
            Approval::Skipped(reason) => {
                outcome.status = OutcomeStatus::Skipped;
                outcome.detail = Some(reason.clone());
            }
            Approval::Approved => {
                if launched_any && limits.dispatch_stagger_ms > 0 {
                    launcher.sleep_ms(limits.dispatch_stagger_ms);
                }
                let (status, detail) = dispatch_one(pr, opener, launcher);
                if status == OutcomeStatus::Opened {
                    launched_any = true;
                }
                outcome.status = status;
                outcome.detail = detail;
            }
        }
        outcomes.push(outcome);
    }

    let aggregate = aggregate(resolution, &outcomes);
    Ok(ExecutionReport {
        outcomes,
        aggregate,
    })
}

/// Build argv for the platform opener. Same reasoning as [`substitute`]: an
/// empty `open_argv` must not leave the target sitting in argv[0].
fn open_with(opener: &OpenerConfig, target: &str) -> Vec<String> {
    if opener.open_argv.is_empty() {
        return Vec::new();
    }
    let mut argv = opener.open_argv.clone();
    argv.push(target.to_owned());
    argv
}

fn dispatch_one(
    pr: &crate::resolver::PlannedResource,
    opener: &OpenerConfig,
    launcher: &dyn Launcher,
) -> (OutcomeStatus, Option<String>) {
    let target = &pr.resource.target;
    match &pr.kind {
        Kind::Https => launch(launcher, &open_with(opener, target), None),
        Kind::File | Kind::Dir => match opener.expand_path(target) {
            Err(e) => (OutcomeStatus::Failed, Some(e)),
            Ok(path) => {
                if !launcher.path_exists(&path) {
                    return (
                        OutcomeStatus::Unavailable,
                        Some(format!("{} does not exist", path.display())),
                    );
                }
                launch(launcher, &open_with(opener, &path.to_string_lossy()), None)
            }
        },
        Kind::Terminal => match opener.expand_path(target) {
            Err(e) => (OutcomeStatus::Failed, Some(e)),
            Ok(path) => {
                if !launcher.path_exists(&path) {
                    return (
                        OutcomeStatus::Unavailable,
                        Some(format!("{} does not exist", path.display())),
                    );
                }
                match &opener.terminal_argv {
                    None => (
                        OutcomeStatus::Failed,
                        Some("no terminal handler configured (murl handler set-terminal)".into()),
                    ),
                    Some(template) => {
                        let argv = substitute(template, &path.to_string_lossy());
                        launch(launcher, &argv, Some(&path))
                    }
                }
            }
        },
        // Remote sessions need an explicitly configured client: guessing a
        // command that receives credentials would be exactly the wrong
        // instinct. Both are DANGEROUS-tier, so they also required trust and
        // consent before reaching here.
        Kind::Ssh => match &opener.ssh_argv {
            None => (
                OutcomeStatus::Failed,
                Some("no ssh handler configured (murl handler set-ssh)".into()),
            ),
            Some(template) => launch(launcher, &substitute(template, target), None),
        },
        Kind::RemoteDesktop => match &opener.remote_desktop_argv {
            None => (
                OutcomeStatus::Failed,
                Some(
                    "no remote-desktop handler configured (murl handler set-remote-desktop)".into(),
                ),
            ),
            Some(template) => launch(launcher, &substitute(template, target), None),
        },
        // Map locations and mail drafts go to the platform opener like any
        // other URI: the OS already knows which app handles them.
        Kind::Geo | Kind::Mailto => launch(launcher, &open_with(opener, target), None),
        // Containers are spliced during resolution and never dispatched;
        // this arm is defense in depth.
        Kind::Murl => (OutcomeStatus::Skipped, Some("container resource".into())),
        Kind::Custom(name) => match opener.custom.get(name) {
            None => (
                OutcomeStatus::Failed,
                Some(format!(
                    "no handler registered for kind custom:{name} (murl handler register)"
                )),
            ),
            Some(template) => {
                let argv = substitute(template, target);
                launch(launcher, &argv, None)
            }
        },
    }
}

fn launch(
    launcher: &dyn Launcher,
    argv: &[String],
    cwd: Option<&Path>,
) -> (OutcomeStatus, Option<String>) {
    // An empty handler template would make `substitute` produce `[target]`,
    // and argv[0] is the *program* — the resource's own target would be
    // executed as a command. `murl handler` refuses to write an empty argv,
    // but handlers.json is a file, so the invariant is enforced where it
    // matters rather than only where it is usually created.
    if argv.is_empty() || argv[0].trim().is_empty() {
        return (
            OutcomeStatus::Failed,
            Some("handler is configured with an empty program; refusing to launch".into()),
        );
    }
    match launcher.launch(argv, cwd) {
        Ok(()) => (OutcomeStatus::Opened, None),
        Err(e) => (OutcomeStatus::Failed, Some(e.to_string())),
    }
}

/// Substitute `{target}` into an argv template. Substitution happens *inside
/// a single argv element* — the target can never become extra arguments, let
/// alone shell syntax. If no element mentions `{target}`, it is appended.
///
/// An empty template yields an empty argv rather than `[target]`, because
/// argv[0] is the program: appending to nothing would run the resource's own
/// target as a command. [`launch`] rejects the empty argv.
fn substitute(template: &[String], target: &str) -> Vec<String> {
    if template.is_empty() {
        return Vec::new();
    }
    let mut argv: Vec<String> = Vec::with_capacity(template.len() + 1);
    let mut used = false;
    for part in template {
        if part.contains("{target}") {
            argv.push(part.replace("{target}", target));
            used = true;
        } else {
            argv.push(part.clone());
        }
    }
    if !used {
        argv.push(target.to_owned());
    }
    argv
}

fn aggregate(resolution: &Resolution, outcomes: &[ResourceOutcome]) -> AggregateStatus {
    let mut any_opened = false;
    let mut any_bad = false;
    let mut any_denied = false;
    let mut required_missed = false;

    for (pr, out) in resolution.resources.iter().zip(outcomes) {
        match out.status {
            OutcomeStatus::Opened => any_opened = true,
            OutcomeStatus::Skipped => {}
            OutcomeStatus::Denied => {
                any_denied = true;
                if pr.resource.required {
                    required_missed = true;
                }
            }
            OutcomeStatus::Failed | OutcomeStatus::Unavailable => {
                any_bad = true;
                if pr.resource.required {
                    required_missed = true;
                }
            }
        }
    }

    if required_missed {
        AggregateStatus::Failed
    } else if any_opened && !any_bad && !any_denied {
        AggregateStatus::Success
    } else if any_opened {
        AggregateStatus::Partial
    } else if any_bad {
        AggregateStatus::Failed
    } else if any_denied {
        AggregateStatus::Denied
    } else {
        // Nothing but skips (e.g. everything deduplicated away).
        AggregateStatus::Success
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitute_is_single_element_only() {
        let argv = substitute(
            &["term".into(), "--dir={target}".into()],
            "/home/u/p; rm -rf ~", // hostile target stays one argument
        );
        assert_eq!(
            argv,
            vec!["term".to_string(), "--dir=/home/u/p; rm -rf ~".to_string()]
        );
        // Appended when the template does not mention it.
        let argv = substitute(&["opener".into()], "https://e.com");
        assert_eq!(
            argv,
            vec!["opener".to_string(), "https://e.com".to_string()]
        );
    }

    #[test]
    fn expand_path() {
        let cfg = OpenerConfig {
            home_dir: Some(PathBuf::from("/home/u")),
            ..OpenerConfig::default()
        };
        assert_eq!(cfg.expand_path("~").unwrap(), PathBuf::from("/home/u"));
        assert_eq!(
            cfg.expand_path("~/p/x").unwrap(),
            PathBuf::from("/home/u/p/x")
        );
        assert_eq!(cfg.expand_path("/abs").unwrap(), PathBuf::from("/abs"));
        // A trailing separator is valid in a manifest, so it must dispatch.
        assert_eq!(cfg.expand_path("~/").unwrap(), PathBuf::from("/home/u"));
        assert_eq!(
            cfg.expand_path("~/projects/").unwrap(),
            PathBuf::from("/home/u/projects")
        );
        // The validator refuses these first; expansion must refuse them too.
        assert!(cfg.expand_path("~//etc/passwd").is_err());
        assert!(cfg.expand_path("~/p//x").is_err());
        assert!(cfg.expand_path("~/../x").is_err());
        let no_home = OpenerConfig::default();
        assert!(no_home.expand_path("~/p").is_err());
    }

    /// On Windows a segment with a drive prefix replaces the home directory
    /// when pushed: `~/Z:\x` would open `Z:\x`.
    #[cfg(windows)]
    #[test]
    fn expand_path_refuses_drive_segments() {
        let cfg = OpenerConfig {
            home_dir: Some(PathBuf::from(r"C:\Users\u")),
            ..OpenerConfig::default()
        };
        for bad in [r"~/Z:\x", "~/Z:x", "~/p/Z:", r"~/p\..\..\x"] {
            assert!(cfg.expand_path(bad).is_err(), "{bad}");
        }
        assert_eq!(
            cfg.expand_path(r"~/p\q").unwrap(),
            PathBuf::from(r"C:\Users\u\p\q")
        );
    }

    #[test]
    fn explorer_detection() {
        for program in [
            "explorer.exe",
            "EXPLORER.EXE",
            "explorer",
            r"C:\Windows\explorer.exe",
            "C:/Windows/Explorer.exe",
        ] {
            assert!(is_explorer(program), "{program}");
        }
        for program in [
            "xdg-open",
            "open",
            "explorer.exe.bak",
            r"C:\tools\myexplorer.exe",
        ] {
            assert!(!is_explorer(program), "{program}");
        }
    }

    #[test]
    fn explorer_arguments_are_one_quoted_token() {
        // The shape that made explorer open a share instead of the browser.
        assert_eq!(
            explorer_argument(r"https://a.example/x,/root,\\host\share").unwrap(),
            r#""https://a.example/x,/root,\\host\share""#
        );
        // Explorer has no backslash escaping: a trailing one stays single.
        assert_eq!(
            explorer_argument(r"C:\Users\u\projects\").unwrap(),
            r#""C:\Users\u\projects\""#
        );
        assert!(explorer_argument(r#"https://a.example/x",/root,\\host\share"#).is_err());
    }
}

#[cfg(test)]
mod status_naming_tests {
    use super::*;

    /// The JSON name, the Display name, and the specification must agree.
    /// They did not once: serde emitted `PARTIAL` while everything else
    /// said `PARTIAL_SUCCESS`, so a JSON consumer matching the documented
    /// string fell through to its error branch and reported failure for a
    /// partially successful activation.
    #[test]
    fn wire_names_match_display_names() {
        for status in [
            AggregateStatus::Success,
            AggregateStatus::Partial,
            AggregateStatus::Failed,
            AggregateStatus::Denied,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            assert_eq!(
                json.trim_matches(0x22 as char),
                status.to_string(),
                "wire and display names differ for {status:?}"
            );
        }
        assert_eq!(
            serde_json::to_string(&AggregateStatus::Partial).unwrap(),
            "\"PARTIAL_SUCCESS\""
        );
    }

    #[test]
    fn outcome_status_wire_names_match_display_names() {
        for status in [
            OutcomeStatus::Opened,
            OutcomeStatus::Skipped,
            OutcomeStatus::Denied,
            OutcomeStatus::Failed,
            OutcomeStatus::Unavailable,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            assert_eq!(json.trim_matches(0x22 as char), status.to_string());
        }
    }
}
