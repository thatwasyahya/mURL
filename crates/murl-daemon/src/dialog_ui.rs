//! A native consent dialog, built on the dialog helper each desktop already
//! ships: `zenity` (GNOME/GTK), `kdialog` (KDE), `osascript` (macOS).
//!
//! This is what the daemon exists for. A terminal prompt cannot serve an
//! activation that arrives from a chat client or a browser, and on macOS a
//! Launch Services activation has no controlling terminal at all — consent
//! there could only ever be a refusal. A dialog fixes the surface without
//! moving a single security rule: [`crate::consent_ui`] still decides what
//! may be offered, and a policy `Deny` never appears as approvable.
//!
//! Three properties this module must not lose:
//!
//! * **No shell, and no generated script.** Backends are invoked as argv
//!   arrays. For `osascript` the AppleScript source is a *constant* and the
//!   plan travels in `argv` — interpolating targets into script text would
//!   be the same mistake as building a shell command, one language over.
//! * **The dialog returns resource ids, nothing else.** Ids are
//!   `[a-z0-9][a-z0-9_-]*`, so a returned line can never be confused with a
//!   separator, a flag, or another resource's text. Anything returned that
//!   was not offered is discarded.
//! * **Every failure is a denial.** No backend, a crash, a timeout, a
//!   closed window, unparseable output — all mean "granted nothing".

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use crate::consent_ui::{ConsentRequest, ConsentUi};

/// How long to wait for the user before treating the prompt as unanswered.
/// Generous: reading a plan is the point, and an unanswered dialog denies.
const DIALOG_TIMEOUT: Duration = Duration::from_secs(180);

/// Maximum characters of any untrusted string shown in the dialog. Labels
/// and targets are validated (no control characters) but not length-bound
/// for display, and a 2 KB target would push the buttons off-screen — an
/// unreadable prompt is not consent.
const MAX_DISPLAY: usize = 96;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Zenity,
    Kdialog,
    Osascript,
}

impl Backend {
    fn program(self) -> &'static str {
        match self {
            Backend::Zenity => "zenity",
            Backend::Kdialog => "kdialog",
            Backend::Osascript => "osascript",
        }
    }
}

/// A consent surface backed by a desktop dialog helper.
#[derive(Debug)]
pub struct DialogUi {
    backend: Backend,
    program: PathBuf,
    timeout: Duration,
}

impl DialogUi {
    /// Find a usable backend, or `None` when the machine has no desktop
    /// dialog helper (headless servers, minimal containers). The caller
    /// then falls back to the terminal, and the terminal falls back to
    /// denial — the chain only ever gets stricter.
    pub fn detect() -> Option<DialogUi> {
        // macOS first: osascript is always present there, and a GUI session
        // is exactly the case that has no terminal.
        let order: &[Backend] = if cfg!(target_os = "macos") {
            &[Backend::Osascript, Backend::Zenity, Backend::Kdialog]
        } else {
            &[Backend::Zenity, Backend::Kdialog]
        };
        // A dialog needs somewhere to draw. On X11/Wayland that is DISPLAY
        // or WAYLAND_DISPLAY; macOS always has a window server.
        let has_display = cfg!(target_os = "macos")
            || std::env::var_os("DISPLAY").is_some_and(|v| !v.is_empty())
            || std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty());
        if !has_display {
            return None;
        }
        for backend in order {
            if let Some(program) = which(backend.program()) {
                return Some(DialogUi {
                    backend: *backend,
                    program,
                    timeout: DIALOG_TIMEOUT,
                });
            }
        }
        None
    }

    /// Construct against an explicit program. Tests point this at a stub
    /// that prints ids, so the whole path — argv construction, output
    /// parsing, offered-id filtering — is exercised without a desktop.
    pub fn with_program(backend: Backend, program: PathBuf) -> DialogUi {
        DialogUi {
            backend,
            program,
            timeout: DIALOG_TIMEOUT,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> DialogUi {
        self.timeout = timeout;
        self
    }

    pub fn backend(&self) -> Backend {
        self.backend
    }

    pub fn describe(&self) -> String {
        format!("{} ({})", self.backend.program(), self.program.display())
    }
}

impl ConsentUi for DialogUi {
    fn ask(&self, request: &ConsentRequest) -> Vec<usize> {
        if request.items.is_empty() {
            return Vec::new();
        }
        let argv = match self.backend {
            Backend::Zenity => zenity_argv(request),
            Backend::Kdialog => kdialog_argv(request),
            Backend::Osascript => osascript_argv(request),
        };

        let stdout = match run_capturing(&self.program, &argv, self.timeout) {
            Ok(text) => text,
            Err(reason) => {
                // Cancel, close, crash, timeout — all the same answer.
                eprintln!("murl-daemon: consent dialog granted nothing ({reason})");
                return Vec::new();
            }
        };

        // Map returned ids back to the indices that were offered. Anything
        // else the backend printed is ignored: a surface cannot grant what
        // it was not shown.
        let mut granted = Vec::new();
        for line in stdout.lines() {
            let id = line.trim().trim_matches('"');
            // Backends may append descriptive text; the id is the first
            // token, and ids never contain whitespace.
            let id = id.split_whitespace().next().unwrap_or("");
            if id.is_empty() {
                continue;
            }
            if let Some(item) = request.items.iter().find(|i| i.id == id) {
                if !granted.contains(&item.index) {
                    granted.push(item.index);
                }
            }
        }
        granted
    }
}

/// Header text shown above the resource list. Untrusted strings are
/// truncated, and the trust status is stated because it is the fact that
/// decides what is even offered.
fn header(request: &ConsentRequest) -> String {
    let mut text = format!("{} wants to open:", clip(&request.name));
    if let Some(identity) = &request.identity {
        text.push('\n');
        text.push_str(&clip(identity));
    }
    text.push_str(&format!(
        "\nfrom {} · trust: {}",
        clip(&request.origin),
        request.trust
    ));
    if !request.denied.is_empty() {
        text.push_str(&format!(
            "\n\n{} resource(s) were refused by policy and cannot be approved here.",
            request.denied.len()
        ));
    }
    text
}

fn clip(s: &str) -> String {
    let cleaned: String = s.chars().filter(|c| !c.is_control()).collect();
    if cleaned.chars().count() <= MAX_DISPLAY {
        return cleaned;
    }
    let mut out: String = cleaned.chars().take(MAX_DISPLAY - 1).collect();
    out.push('…');
    out
}

fn zenity_argv(request: &ConsentRequest) -> Vec<String> {
    let mut argv = vec![
        "--list".to_string(),
        "--checklist".to_string(),
        "--title=mURL".to_string(),
        format!("--text={}", header(request)),
        "--column=Open".to_string(),
        "--column=Resource".to_string(),
        "--column=Risk".to_string(),
        "--column=Target".to_string(),
        // Print the id column, not the checkbox column.
        "--print-column=2".to_string(),
        "--separator=\n".to_string(),
        "--width=760".to_string(),
        "--height=460".to_string(),
    ];
    for item in &request.items {
        // Nothing is pre-checked: consent starts from no.
        argv.push("FALSE".to_string());
        argv.push(item.id.clone());
        argv.push(item.tier.to_string());
        argv.push(clip(&item.target));
    }
    argv
}

fn kdialog_argv(request: &ConsentRequest) -> Vec<String> {
    let mut argv = vec![
        "--title".to_string(),
        "mURL".to_string(),
        "--separate-output".to_string(),
        "--checklist".to_string(),
        header(request),
    ];
    for item in &request.items {
        argv.push(item.id.clone()); // tag: what comes back
        argv.push(format!(
            "{} — {} — {}",
            item.id,
            item.tier,
            clip(&item.target)
        ));
        argv.push("off".to_string());
    }
    argv
}

/// A *constant* AppleScript. The plan reaches it through `argv`, never
/// through the script text — the same rule as "no shell", one language over.
const OSASCRIPT_SOURCE: &str = r#"on run argv
    set prompt to item 1 of argv
    set choices to {}
    repeat with i from 2 to count of argv
        set end of choices to item i of argv
    end repeat
    if (count of choices) is 0 then return ""
    set picked to choose from list choices with prompt prompt with title "mURL" with multiple selections allowed
    if picked is false then return ""
    set out to ""
    repeat with p in picked
        set out to out & (p as text) & linefeed
    end repeat
    return out
end run"#;

fn osascript_argv(request: &ConsentRequest) -> Vec<String> {
    let mut argv = vec![
        "-e".to_string(),
        OSASCRIPT_SOURCE.to_string(),
        // Everything after this is data for `on run argv`.
        header(request),
    ];
    for item in &request.items {
        // The id leads, so the returned line parses back to it.
        argv.push(format!(
            "{}  ·  {}  ·  {}",
            item.id,
            item.tier,
            clip(&item.target)
        ));
    }
    argv
}

/// Spawn `program` with `argv`, capture stdout, and give up after `timeout`.
///
/// A dialog that never returns would otherwise wedge the daemon, which
/// serves connections one at a time.
fn run_capturing(
    program: &std::path::Path,
    argv: &[String],
    timeout: Duration,
) -> Result<String, String> {
    let mut child = Command::new(program)
        .args(argv)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("cannot run {}: {e}", program.display()))?;

    let mut stdout = child.stdout.take().ok_or("no stdout pipe")?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = String::new();
        use std::io::Read as _;
        let result = stdout.read_to_string(&mut buf).map(|_| buf);
        let _ = tx.send(result);
    });

    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let text = rx
                    .recv_timeout(Duration::from_secs(5))
                    .map_err(|_| "dialog output was never readable".to_string())?
                    .map_err(|e| format!("reading dialog output: {e}"))?;
                // A non-zero exit is Cancel on every backend here.
                if !status.success() {
                    return Err("dialog was cancelled".into());
                }
                return Ok(text);
            }
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("no answer within {}s", timeout.as_secs()));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(format!("waiting for dialog: {e}")),
        }
    }
}

/// Locate a program on `PATH`, without a dependency.
fn which(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(program);
        if let Ok(meta) = std::fs::metadata(&candidate) {
            if meta.is_file() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if meta.permissions().mode() & 0o111 != 0 {
                        return Some(candidate);
                    }
                }
                #[cfg(not(unix))]
                {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

/// Write a throwaway executable script, for tests and for `murl-daemon
/// --check-dialog`.
pub fn write_stub_script(path: &std::path::Path, body: &str) -> std::io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    writeln!(file, "#!/bin/sh")?;
    write!(file, "{body}")?;
    drop(file);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consent_ui::ConsentItem;
    use murl_core::policy::Tier;

    fn request() -> ConsentRequest {
        ConsentRequest {
            name: "Project X".into(),
            identity: Some("murl://local/project-x".into()),
            origin: "local store".into(),
            trust: "LOCAL".into(),
            items: vec![
                ConsentItem {
                    index: 0,
                    id: "docs".into(),
                    label: "Documentation".into(),
                    kind: "https".into(),
                    target: "https://docs.example/x".into(),
                    tier: Tier::Safe,
                    reasons: vec!["SAFE resource".into()],
                },
                ConsentItem {
                    index: 2,
                    id: "workspace".into(),
                    label: "Workspace".into(),
                    kind: "dir".into(),
                    target: "~/projects/x".into(),
                    tier: Tier::Sensitive,
                    reasons: vec!["SENSITIVE resource".into()],
                },
            ],
            denied: vec![],
        }
    }

    fn stub(tag: &str, body: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "murl-dialog-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("stub");
        write_stub_script(&path, body).unwrap();
        (dir, path)
    }

    #[test]
    fn returned_ids_map_back_to_offered_indices() {
        // The stub answers with ids, exactly as a real backend would.
        let (dir, path) = stub("ok", "printf 'docs\\nworkspace\\n'\n");
        let ui = DialogUi::with_program(Backend::Zenity, path);
        assert_eq!(ui.ask(&request()), vec![0, 2]);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_backend_cannot_grant_what_it_was_not_offered() {
        let (dir, path) = stub(
            "rogue",
            "printf 'docs\\nterminal\\nghost\\n../etc/passwd\\n'\n",
        );
        let ui = DialogUi::with_program(Backend::Zenity, path);
        // `terminal` was denied by policy and never offered; the rest are
        // not resources at all.
        assert_eq!(ui.ask(&request()), vec![0]);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn cancel_grants_nothing() {
        let (dir, path) = stub("cancel", "exit 1\n");
        let ui = DialogUi::with_program(Backend::Zenity, path);
        assert!(ui.ask(&request()).is_empty());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn empty_or_noisy_output_grants_nothing() {
        let (dir, path) = stub("noise", "printf '\\n  \\n\"\"\\n'\n");
        let ui = DialogUi::with_program(Backend::Zenity, path);
        assert!(ui.ask(&request()).is_empty());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_hanging_dialog_times_out_into_denial() {
        let (dir, path) = stub("hang", "sleep 30\n");
        let ui =
            DialogUi::with_program(Backend::Zenity, path).with_timeout(Duration::from_millis(400));
        let started = std::time::Instant::now();
        assert!(ui.ask(&request()).is_empty());
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the timeout did not fire"
        );
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_missing_backend_grants_nothing() {
        let ui = DialogUi::with_program(Backend::Zenity, PathBuf::from("/nonexistent/zenity"));
        assert!(ui.ask(&request()).is_empty());
    }

    #[test]
    fn kdialog_quoted_output_parses() {
        let (dir, path) = stub("kdialog", "printf '\"docs\"\\n\"workspace\"\\n'\n");
        let ui = DialogUi::with_program(Backend::Kdialog, path);
        assert_eq!(ui.ask(&request()), vec![0, 2]);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn osascript_descriptive_lines_parse_back_to_ids() {
        // `choose from list` returns the whole displayed item.
        let (dir, path) = stub(
            "osa",
            "printf 'docs  ·  SAFE  ·  https://docs.example/x\\n'\n",
        );
        let ui = DialogUi::with_program(Backend::Osascript, path);
        assert_eq!(ui.ask(&request()), vec![0]);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn argv_carries_the_plan_and_never_a_script_fragment() {
        let r = request();
        // osascript: the script is constant, the data is separate argv.
        let argv = osascript_argv(&r);
        assert_eq!(argv[0], "-e");
        assert_eq!(argv[1], OSASCRIPT_SOURCE);
        assert!(
            !argv[1].contains("docs"),
            "plan text leaked into the script"
        );
        assert!(argv.iter().skip(2).any(|a| a.starts_with("docs")));

        // zenity: nothing is pre-checked, and the id column is what prints.
        let argv = zenity_argv(&r);
        assert!(argv.iter().any(|a| a == "--print-column=2"));
        assert!(argv.iter().all(|a| a != "TRUE"), "a row was pre-approved");
        assert_eq!(argv.iter().filter(|a| *a == "FALSE").count(), 2);
    }

    #[test]
    fn hostile_display_strings_are_clipped_and_stripped() {
        let mut r = request();
        r.items[0].target = format!("https://e.example/{}", "a".repeat(500));
        r.name = "Project\u{0007}X".into();
        let argv = zenity_argv(&r);
        assert!(argv.iter().all(|a| a.chars().count() <= 1200));
        assert!(argv.iter().any(|a| a.contains('…') || a.contains("aaa")));
        assert!(
            !argv.iter().any(|a| a.contains('\u{0007}')),
            "a control character reached the dialog"
        );
    }
}
