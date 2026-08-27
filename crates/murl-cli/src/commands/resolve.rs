//! `murl resolve` — resolve to a full dispatch plan without opening anything.

use murl_core::Result;

use crate::ctx::App;
use crate::render;

pub fn run(app: &App, target: &str) -> Result<i32> {
    let mut resolution = app.resolve_target(target)?;
    resolution.apply_policy(&app.policy);

    if app.json {
        println!("{}", serde_json::to_string_pretty(&resolution.to_json())?);
    } else {
        print!("{}", render::plan_text(&resolution));
    }
    Ok(0)
}
