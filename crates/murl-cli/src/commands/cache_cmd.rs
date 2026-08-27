//! `murl cache` — inspect and manage the remote-manifest cache.

use murl_core::error::{Error, Result};
use murl_core::murl::Murl;

use crate::ctx::App;

pub fn list(app: &App) -> Result<i32> {
    let entries = app.cache.list()?;
    if app.json {
        println!("{}", serde_json::to_string_pretty(&entries)?);
    } else if entries.is_empty() {
        println!("(cache is empty)");
    } else {
        for e in entries {
            println!("{}  fetched_at={}  {}", e.identity, e.fetched_at, e.url);
        }
    }
    Ok(0)
}

pub fn evict(app: &App, murl: &str) -> Result<i32> {
    let m = Murl::parse(murl)?;
    if app.cache.evict(&m.identity())? {
        println!("evicted {}", m.identity());
        Ok(0)
    } else {
        Err(Error::NotFound(format!("{} is not cached", m.identity())))
    }
}

pub fn clear(app: &App) -> Result<i32> {
    let n = app.cache.clear()?;
    println!(
        "cleared {n} cached manifest{}",
        if n == 1 { "" } else { "s" }
    );
    Ok(0)
}
