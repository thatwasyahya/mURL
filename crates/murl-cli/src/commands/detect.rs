//! `murl handler detect` — find installed programs that could serve the
//! handler-gated kinds, and print the commands that would register them.
//!
//! **It never registers anything.** The security model's central asymmetry
//! is that manifests name capabilities while only the user maps names to
//! programs (docs/security.md); a convenience that silently wired up a
//! terminal would erase exactly that. So this command's entire output is
//! copy-pasteable `murl handler set-…` lines, and the user runs the ones
//! they want.

use std::path::PathBuf;

use murl_core::error::Result;
use serde_json::json;

use crate::ctx::{load_handlers, App};

/// A program worth suggesting, with the argv template that drives it.
struct Candidate {
    program: &'static str,
    /// Argv after the program, with `{target}` where the target belongs.
    args: &'static [&'static str],
    note: &'static str,
}

const TERMINALS: &[Candidate] = &[
    Candidate {
        program: "wezterm",
        args: &["start", "--cwd", "{target}"],
        note: "",
    },
    Candidate {
        program: "kitty",
        args: &["--directory", "{target}"],
        note: "",
    },
    Candidate {
        program: "alacritty",
        args: &["--working-directory", "{target}"],
        note: "",
    },
    Candidate {
        program: "gnome-terminal",
        args: &["--working-directory={target}"],
        note: "",
    },
    Candidate {
        program: "konsole",
        args: &["--workdir", "{target}"],
        note: "",
    },
    Candidate {
        program: "xfce4-terminal",
        args: &["--working-directory={target}"],
        note: "",
    },
    Candidate {
        program: "foot",
        args: &["--working-directory={target}"],
        note: "",
    },
    Candidate {
        program: "x-terminal-emulator",
        args: &[],
        note: "Debian alternatives symlink; the concrete terminal may take a different flag",
    },
    Candidate {
        program: "wt.exe",
        args: &["-d", "{target}"],
        note: "Windows Terminal",
    },
];

const SSH_CLIENTS: &[Candidate] = &[
    Candidate {
        program: "wezterm",
        args: &["start", "--", "ssh", "{target}"],
        note: "runs ssh inside a new window",
    },
    Candidate {
        program: "kitty",
        args: &["ssh", "{target}"],
        note: "",
    },
    Candidate {
        program: "x-terminal-emulator",
        args: &["-e", "ssh", "{target}"],
        note: "",
    },
    Candidate {
        program: "ssh",
        args: &["{target}"],
        note: "only useful when murl itself runs in a terminal",
    },
];

const REMOTE_DESKTOP_CLIENTS: &[Candidate] = &[
    Candidate {
        program: "xfreerdp",
        args: &["{target}"],
        note: "rdp:// targets",
    },
    Candidate {
        program: "remmina",
        args: &["-c", "{target}"],
        note: "rdp:// and vnc://",
    },
    Candidate {
        program: "vinagre",
        args: &["{target}"],
        note: "vnc://",
    },
    Candidate {
        program: "mstsc.exe",
        args: &[],
        note: "Windows RDP client; takes a .rdp file, not a URL — needs a wrapper",
    },
];

pub fn run(app: &App) -> Result<i32> {
    let groups = [
        ("terminal", "set-terminal", TERMINALS),
        ("ssh", "set-ssh", SSH_CLIENTS),
        (
            "remote-desktop",
            "set-remote-desktop",
            REMOTE_DESKTOP_CLIENTS,
        ),
    ];
    let configured = load_handlers(&app.paths.handlers_file())?;

    let mut found: Vec<(&str, &str, String, Vec<String>, &str)> = Vec::new();
    for (kind, subcommand, candidates) in groups {
        for candidate in candidates {
            if let Some(path) = which(candidate.program) {
                let argv: Vec<String> = std::iter::once(candidate.program.to_string())
                    .chain(candidate.args.iter().map(|a| a.to_string()))
                    .collect();
                found.push((
                    kind,
                    subcommand,
                    path.display().to_string(),
                    argv,
                    candidate.note,
                ));
            }
        }
    }

    if app.json {
        let items: Vec<_> = found
            .iter()
            .map(|(kind, subcommand, path, argv, note)| {
                json!({
                    "kind": kind,
                    "path": path,
                    "argv": argv,
                    "note": note,
                    "command": format!("murl handler {subcommand} -- {}", argv.join(" ")),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "detected": items,
                "configured": {
                    "terminal": configured.terminal,
                    "ssh": configured.ssh,
                    "remoteDesktop": configured.remote_desktop,
                },
            }))?
        );
        return Ok(0);
    }

    if found.is_empty() {
        println!("No known handler programs found on PATH.");
        println!("Configure one manually, e.g.:");
        println!("  murl handler set-terminal -- <your-terminal> --working-directory={{target}}");
        return Ok(0);
    }

    println!("Found these programs. Nothing has been registered — run the line you want:\n");
    let mut current = "";
    for (kind, subcommand, path, argv, note) in &found {
        if *kind != current {
            let already = match *kind {
                "terminal" => configured.terminal.as_ref(),
                "ssh" => configured.ssh.as_ref(),
                _ => configured.remote_desktop.as_ref(),
            };
            match already {
                Some(argv) => println!("{kind} (currently: {argv:?})"),
                None => println!("{kind} (currently unset)"),
            }
            current = kind;
        }
        println!("  murl handler {subcommand} -- {}", argv.join(" "));
        println!(
            "      {path}{}",
            if note.is_empty() {
                String::new()
            } else {
                format!("  — {note}")
            }
        );
    }
    println!("\nAll three kinds are DANGEROUS-tier: registering a handler makes them");
    println!("dispatchable, but they still require trust and consent at activation.");
    Ok(0)
}

/// Locate a program on `PATH`. A small `which`, so the CLI keeps its
/// dependency set as small as its threat model likes.
fn which(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(program);
        if is_executable(&candidate) {
            return Some(candidate);
        }
        // On Windows the caller may name `wt.exe` explicitly; also try the
        // PATHEXT-style `.exe` suffix when no extension was given.
        if cfg!(windows) && !program.contains('.') {
            let with_exe = dir.join(format!("{program}.exe"));
            if is_executable(&with_exe) {
                return Some(with_exe);
            }
        }
    }
    None
}

fn is_executable(path: &std::path::Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn which_finds_a_real_program_and_misses_a_fake_one() {
        // `sh` exists on every unix; nothing is named this.
        #[cfg(unix)]
        assert!(which("sh").is_some());
        assert!(which("definitely-not-a-real-program-9f3a").is_none());
    }

    #[test]
    fn every_candidate_template_mentions_the_target_or_is_documented() {
        // A template with no {target} appends the target as the last
        // argument (dispatch.rs::substitute). That is fine for some
        // programs and wrong for others, so anything without {target} must
        // carry a note explaining itself.
        for group in [TERMINALS, SSH_CLIENTS, REMOTE_DESKTOP_CLIENTS] {
            for candidate in group {
                let has_target = candidate.args.iter().any(|a| a.contains("{target}"));
                assert!(
                    has_target || !candidate.note.is_empty(),
                    "candidate `{}` has no {{target}} and no explanatory note",
                    candidate.program
                );
            }
        }
    }
}
