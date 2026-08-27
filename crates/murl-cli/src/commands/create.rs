//! `murl create` — write a starter manifest.

use murl_core::error::{Error, Result};
use murl_core::manifest::Manifest;
use serde_json::json;

use crate::ctx::App;

pub fn run(app: &App, name: Option<&str>, output: Option<&str>, force: bool) -> Result<i32> {
    let name = name.unwrap_or("My Project");
    let slug = slugify(name);
    let template = json!({
        "murlVersion": "0.1",
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
    });
    let bytes = serde_json::to_vec_pretty(&template)?;

    // The template must always pass our own validator.
    let manifest = Manifest::from_slice(&bytes, &app.limits)?;
    let report = manifest.validate();
    debug_assert!(
        report.is_valid(),
        "template failed validation: {:?}",
        report.errors
    );

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
            println!("  1. edit the resources to match your destination");
            println!("  2. murl validate {}", path.display());
            println!("  3. murl name add {slug} {}", path.display());
            println!("  4. murl open murl://local/{slug}");
        }
    }
    Ok(0)
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
    use super::slugify;

    #[test]
    fn slugs() {
        assert_eq!(slugify("Project X"), "project-x");
        assert_eq!(slugify("  --Weird__Name!!"), "weird-name");
        assert_eq!(slugify("!!!"), "destination");
    }
}
