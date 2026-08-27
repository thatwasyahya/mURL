//! `murl parse` — parse an mURL and show its components.

use murl_core::fetch::well_known_url;
use murl_core::murl::Murl;
use murl_core::Result;
use serde_json::json;

use crate::ctx::App;

pub fn run(app: &App, murl_str: &str) -> Result<i32> {
    let m = Murl::parse(murl_str)?;
    if app.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "input": murl_str,
                "identity": m.identity(),
                "canonical": m.to_string(),
                "authority": m.authority.to_string(),
                "local": m.authority.is_local(),
                "name": m.name,
                "version": m.version.to_string(),
                "selector": m.selector_display(),
                "selectorItems": m.selector,
                "query": m.query,
                "wellKnownUrl": well_known_url(&m),
            }))?
        );
    } else {
        println!("identity:   {}", m.identity());
        println!(
            "authority:  {} ({})",
            m.authority,
            if m.authority.is_local() {
                "local store"
            } else {
                "remote"
            }
        );
        println!("name:       {}", m.name_path());
        println!("version:    {}", m.version);
        if let Some(sel) = m.selector_display() {
            println!("selector:   #{sel}");
        }
        if let Some(q) = &m.query {
            println!("query:      ?{q}");
        }
        if let Some(url) = well_known_url(&m) {
            println!("manifest:   {url}");
        }
    }
    Ok(0)
}
