//! A terminal implementation of [`ConsentUi`].
//!
//! This is the daemon's fallback surface and the reference for what any GUI
//! must show: the destination's identity, where its manifest came from, its
//! trust status, every resource with its tier, and — separately, as
//! information rather than choices — what policy already refused.
//!
//! Without a TTY it grants nothing (threat D-2: an unattended daemon must
//! not approve on the user's behalf).

use std::io::{BufRead, IsTerminal, Write};

use crate::consent_ui::{ConsentRequest, ConsentUi};

#[derive(Debug, Default)]
pub struct TerminalUi;

impl ConsentUi for TerminalUi {
    fn ask(&self, request: &ConsentRequest) -> Vec<usize> {
        if request.items.is_empty() {
            return Vec::new();
        }
        if !(std::io::stdin().is_terminal() && std::io::stderr().is_terminal()) {
            eprintln!(
                "murl-daemon: {} resource(s) need consent but no terminal is attached; denying",
                request.items.len()
            );
            return Vec::new();
        }

        let mut err = std::io::stderr();
        let _ = writeln!(err, "\n{} wants to open:", request.name);
        if let Some(identity) = &request.identity {
            let _ = writeln!(err, "  {identity}");
        }
        let _ = writeln!(err, "  from {} · trust: {}", request.origin, request.trust);
        let _ = writeln!(err);
        for item in &request.items {
            let _ = writeln!(
                err,
                "  [{}] {:9} {:9} {}",
                item.index, item.tier, item.kind, item.target
            );
            if !item.reasons.is_empty() {
                let _ = writeln!(err, "        {}", item.reasons.join("; "));
            }
        }
        if !request.denied.is_empty() {
            let _ = writeln!(err, "\n  refused by policy (cannot be approved here):");
            for (item, reason) in &request.denied {
                let _ = writeln!(err, "    ✗ {:9} {}  — {reason}", item.tier, item.target);
            }
        }

        let _ = write!(err, "\nOpen? [a]ll / indices like 0,2 / [N]one: ");
        let _ = err.flush();
        let mut line = String::new();
        let _ = std::io::stdin().lock().read_line(&mut line);
        parse_answer(&line, request)
    }
}

/// Parse the answer. Unknown input means none — the safe reading of
/// ambiguity is refusal.
fn parse_answer(line: &str, request: &ConsentRequest) -> Vec<usize> {
    let answer = line.trim().to_ascii_lowercase();
    if answer == "a" || answer == "all" {
        return request.items.iter().map(|i| i.index).collect();
    }
    if answer.is_empty() || answer == "n" || answer == "none" {
        return Vec::new();
    }
    // A list of indices; anything unparseable or not offered is ignored.
    let offered: Vec<usize> = request.items.iter().map(|i| i.index).collect();
    answer
        .split(',')
        .filter_map(|part| part.trim().parse::<usize>().ok())
        .filter(|i| offered.contains(i))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consent_ui::ConsentItem;
    use murl_core::policy::Tier;

    fn request_with(indices: &[usize]) -> ConsentRequest {
        ConsentRequest {
            name: "T".into(),
            identity: Some("murl://local/t".into()),
            origin: "local store".into(),
            trust: "LOCAL".into(),
            items: indices
                .iter()
                .map(|&index| ConsentItem {
                    index,
                    id: format!("r{index}"),
                    label: format!("R{index}"),
                    kind: "https".into(),
                    target: format!("https://e.com/{index}"),
                    tier: Tier::Safe,
                    reasons: vec![],
                })
                .collect(),
            denied: vec![],
        }
    }

    #[test]
    fn all_and_none() {
        let r = request_with(&[0, 1, 2]);
        assert_eq!(parse_answer("a\n", &r), vec![0, 1, 2]);
        assert_eq!(parse_answer("all", &r), vec![0, 1, 2]);
        assert!(parse_answer("", &r).is_empty());
        assert!(parse_answer("n", &r).is_empty());
        assert!(parse_answer("\n", &r).is_empty());
    }

    #[test]
    fn index_lists() {
        let r = request_with(&[0, 1, 2]);
        assert_eq!(parse_answer("0,2", &r), vec![0, 2]);
        assert_eq!(parse_answer(" 1 , 2 ", &r), vec![1, 2]);
    }

    #[test]
    fn unoffered_and_garbage_indices_are_dropped() {
        let r = request_with(&[0, 2]); // index 1 was decided by policy
        assert_eq!(parse_answer("0,1,2", &r), vec![0, 2]);
        assert!(parse_answer("yes please", &r).is_empty());
        assert!(parse_answer("999", &r).is_empty());
        assert!(parse_answer("-1", &r).is_empty());
    }
}
