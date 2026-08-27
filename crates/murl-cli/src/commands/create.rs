//! `murl create` — write a starter manifest, from a template or by asking.

use std::io::{BufRead, IsTerminal, Write};

use murl_core::error::{Error, Result};
use murl_core::kind::Kind;
use murl_core::manifest::Manifest;
use murl_core::policy::classify;
use serde_json::{json, Value};

use crate::ctx::App;

pub fn run(
    app: &App,
    name: Option<&str>,
    output: Option<&str>,
    force: bool,
    interactive: bool,
) -> Result<i32> {
    let (doc, slug) = if interactive {
        if !std::io::stdin().is_terminal() {
            return Err(Error::Validation(
                "--interactive needs a terminal; drop the flag to use the template".into(),
            ));
        }
        build_interactively(name)?
    } else {
        let name = name.unwrap_or("My Project");
        (template(name), slugify(name))
    };

    let bytes = serde_json::to_vec_pretty(&doc)?;

    // Whatever we produce must pass our own validator before it is written.
    let manifest = Manifest::from_slice(&bytes, &app.limits)?;
    let report = manifest.validate();
    if !report.is_valid() {
        for issue in &report.errors {
            eprintln!("error {}: {}", issue.path, issue.message);
        }
        return Err(Error::Validation(
            "the generated manifest is invalid; please report this as a bug".into(),
        ));
    }

    match output {
        Some("-") => {
            let mut text = String::from_utf8(bytes).expect("template is UTF-8");
            text.push('\n');
            print!("{text}");
        }
        other => {
            let path = other
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::path::PathBuf::from(format!("{slug}.murl.json")));
            if path.exists() && !force {
                return Err(Error::Validation(format!(
                    "{} already exists (pass --force to overwrite)",
                    path.display()
                )));
            }
            let mut data = bytes;
            data.push(b'\n');
            std::fs::write(&path, data)?;
            println!("created {}", path.display());
            println!();
            println!("next steps:");
            println!("  1. murl validate {}", path.display());
            println!("  2. murl name add {slug} {}", path.display());
            println!("  3. murl open murl://local/{slug}");
        }
    }
    Ok(0)
}

fn template(name: &str) -> Value {
    let slug = slugify(name);
    json!({
        "murlVersion": "0.2",
        "id": format!("murl://local/{slug}"),
        "name": name,
        "description": format!("{name} — everything this destination is made of."),
        "resources": [
            {
                "id": "docs",
                "kind": "https",
                "target": "https://example.com/docs",
                "label": "Documentation",
                "role": "docs",
                "order": 10
            },
            {
                "id": "workspace",
                "kind": "dir",
                "target": format!("~/projects/{slug}"),
                "label": "Local workspace",
                "role": "workspace",
                "order": 20
            }
        ]
    })
}

/// Ask for a name, then resources until an empty target ends the loop.
/// Every answer is validated as it is entered — the manifest that comes out
/// is one that resolves, not one the author has to debug afterwards.
fn build_interactively(name_arg: Option<&str>) -> Result<(Value, String)> {
    let mut err = std::io::stderr();
    let _ = writeln!(
        err,
        "Creating a destination. Press Enter to accept [defaults]; an empty target ends the list.\n"
    );

    let name = match name_arg {
        Some(n) => n.to_owned(),
        None => {
            let n = ask("Destination name", Some("My Project"));
            if n.is_empty() {
                "My Project".to_owned()
            } else {
                n
            }
        }
    };
    let slug = slugify(&name);
    let description = ask("Description (optional)", None);

    let mut resources: Vec<Value> = Vec::new();
    let mut order = 10;
    loop {
        let _ = writeln!(
            err,
            "\nResource {} (leave target empty to finish)",
            resources.len() + 1
        );
        let target = ask("  target", None);
        if target.is_empty() {
            break;
        }

        let kind_default = guess_kind(&target);
        let kind_str = loop {
            let entered = ask(
                "  kind (https/file/dir/murl/terminal/custom:x)",
                Some(kind_default),
            );
            let candidate = if entered.is_empty() {
                kind_default.to_owned()
            } else {
                entered
            };
            match Kind::parse(&candidate) {
                Ok(_) => break candidate,
                Err(e) => {
                    let _ = writeln!(err, "    ! {e}");
                }
            }
        };
        let kind = Kind::parse(&kind_str).expect("validated above");

        // Show the risk tier before the author commits to the resource.
        let tier = classify(&kind, &target);
        let _ = writeln!(err, "    → classified {tier}");

        let default_id = default_id_for(&target, resources.len());
        let id = loop {
            let entered = ask("  id", Some(&default_id));
            let candidate = if entered.is_empty() {
                default_id.clone()
            } else {
                entered
            };
            if !murl_core::manifest::is_valid_resource_id(&candidate) {
                let _ = writeln!(err, "    ! ids must match [a-z0-9][a-z0-9_-]{{0,63}}");
                continue;
            }
            if resources.iter().any(|r| r["id"] == json!(candidate)) {
                let _ = writeln!(err, "    ! id `{candidate}` is already used");
                continue;
            }
            break candidate;
        };

        let label = ask("  label (optional)", None);
        let role = ask("  role (optional, e.g. source/docs/issues)", None);

        let mut resource = json!({
            "id": id,
            "kind": kind_str,
            "target": target,
            "order": order,
        });
        if !label.is_empty() {
            resource["label"] = json!(label);
        }
        if !role.is_empty() {
            resource["role"] = json!(role);
        }
        resources.push(resource);
        order += 10;
    }

    if resources.is_empty() {
        return Err(Error::Validation(
            "a destination needs at least one resource".into(),
        ));
    }

    let mut doc = json!({
        "murlVersion": "0.2",
        "id": format!("murl://local/{slug}"),
        "name": name,
        "resources": resources,
    });
    if !description.is_empty() {
        doc["description"] = json!(description);
    }
    Ok((doc, slug))
}

fn ask(prompt: &str, default: Option<&str>) -> String {
    let mut err = std::io::stderr();
    match default {
        Some(d) => {
            let _ = write!(err, "{prompt} [{d}]: ");
        }
        None => {
            let _ = write!(err, "{prompt}: ");
        }
    }
    let _ = err.flush();
    let mut line = String::new();
    let _ = std::io::stdin().lock().read_line(&mut line);
    line.trim().to_owned()
}

fn guess_kind(target: &str) -> &'static str {
    if target.starts_with("https://") || target.starts_with("http://") {
        "https"
    } else if target.starts_with("murl:") {
        "murl"
    } else if target.ends_with('/')
        || !target
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or("")
            .contains('.')
    {
        "dir"
    } else {
        "file"
    }
}

/// Suggest an id from the target's most distinctive path component. The
/// scheme is dropped first so `https://…` never suggests the id "https".
fn default_id_for(target: &str, index: usize) -> String {
    let body = target.split_once("://").map_or(target, |(_, rest)| rest);
    let trimmed = body.trim_end_matches('/');
    let last = trimmed.rsplit(['/', '\\']).next().unwrap_or("");
    let stem = last.split('.').next().unwrap_or(last);
    let slug = slugify(stem);
    if slug.is_empty() || slug == "destination" {
        format!("resource-{}", index + 1)
    } else {
        slug
    }
}

fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_dash = true; // suppress leading dashes
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        out.push_str("destination");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slugify("Project X"), "project-x");
        assert_eq!(slugify("  --Weird__Name!!"), "weird-name");
        assert_eq!(slugify("!!!"), "destination");
    }

    #[test]
    fn kind_guessing() {
        assert_eq!(guess_kind("https://example.com"), "https");
        assert_eq!(guess_kind("murl://local/x"), "murl");
        assert_eq!(guess_kind("~/projects/x"), "dir");
        assert_eq!(guess_kind("/home/u/notes.txt"), "file");
    }

    #[test]
    fn id_suggestions() {
        assert_eq!(
            default_id_for("https://github.com/acme/project-x", 0),
            "project-x"
        );
        assert_eq!(default_id_for("/home/u/notes.txt", 0), "notes");
        // Bare host: the stem before the first dot is the useful part.
        assert_eq!(default_id_for("https://example.com/", 2), "example");
        // Nothing usable falls back to a positional id.
        assert_eq!(default_id_for("https://", 2), "resource-3");
    }

    #[test]
    fn template_is_valid() {
        let bytes = serde_json::to_vec(&template("Project X")).unwrap();
        let m = Manifest::from_slice(&bytes, &murl_core::Limits::default()).unwrap();
        let report = m.validate();
        assert!(report.is_valid(), "{:?}", report.errors);
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    }
}
