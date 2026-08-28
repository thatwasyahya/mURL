#!/usr/bin/env bash
# Seed the issue tracker with real, scoped work.
#
# Run this once, after the repository is public:  bash .github/seed-issues.sh
#
# Every issue below is a genuine open item from docs/roadmap.md — none are
# invented to look busy. Each states where to start and how to know it is
# done, because "good first issue" without those is just a label.

set -euo pipefail

command -v gh >/dev/null || { echo "needs the GitHub CLI (gh)"; exit 1; }
REPO="${1:-thatwasyahya/mURL}"

echo "Seeding issues in $REPO"

label() {
    gh label create "$1" --repo "$REPO" --color "$2" --description "$3" 2>/dev/null || true
}
label "good first issue" "7057ff" "Scoped, with a clear finish line"
label "help wanted"      "008672" "Would benefit from someone picking it up"
label "security"         "b60205" "Touches the security model"
label "spec"             "0e8a16" "Changes or clarifies the specification"
label "platform"         "1d76db" "Platform-specific work"

issue() {
    local title="$1" body="$2" labels="$3"
    gh issue create --repo "$REPO" --title "$title" --body "$body" --label "$labels" \
        && echo "  created: $title"
}

issue "Windows: named-pipe transport for the daemon" \
"The daemon speaks over a Unix socket. On Windows the console path works, but
there is no transport, so \`murl open\` never routes through the daemon there.

**Where to start:** \`crates/murl-daemon/src/socket.rs\` already returns a
named-pipe path on Windows; \`server.rs\` and \`client.rs\` have Unix-only
implementations behind \`#[cfg(unix)]\`.

**Done when:** \`murl-daemon run\` serves on Windows, \`murl --daemon open\`
reaches it, and the tests in \`tests/daemon_flow.rs\` pass there — they are
transport-free by design, so they should need no changes.

**Keep:** the security properties in \`docs/daemon.md\` D-1 and D-5 — the pipe
must be user-private, and the client must verify before trusting it." \
"help wanted,platform"

issue "Background cache refresh and expiry notifications" \
"Cached remote manifests go stale silently. A resident daemon could refresh
them before they are needed and tell the user when a destination they use is
about to expire.

**Where to start:** \`crates/murl-core/src/cache.rs\` (TTL and freshness are
already modelled), \`crates/murl-daemon/src/server.rs\`.

**Done when:** the daemon refreshes entries approaching their TTL without
blocking an activation, and never fetches for a name the user has not used.

**Careful:** refreshing must not become a background tracker. Fetch only what
is already cached, and never on a schedule tight enough to profile the user." \
"help wanted"

issue "Localize the consent surfaces" \
"The consent dialog and terminal prompt are English-only. Consent the user
cannot read is not consent.

**Where to start:** \`crates/murl-daemon/src/dialog_ui.rs\` and
\`terminal_ui.rs\` build every string inline.

**Done when:** strings come from one place, at least one non-English locale
exists, and the tier names stay legible in it.

**Constraint:** no heavy i18n dependency — the dependency policy in
CONTRIBUTING.md applies." \
"good first issue,help wanted"

issue "More conformance vectors" \
"\`spec/conformance/\` is how a second implementation checks itself. It grew a
canonical-form section only after a second implementation passed everything
else with an untested canonical form — that is the kind of hole worth hunting.

**Where to start:** \`spec/conformance/README.md\` describes the five rules and
the layout. Add a vector, run both harnesses (\`cargo test -p murl-core --test
conformance\` and \`reference/python/run_conformance.py\`).

**Especially wanted:** cases where you expected one behaviour and got another.
A vector the implementations disagree on is the most valuable thing you can
contribute — open it as a bug, not as a fix." \
"good first issue,spec"

issue "A third implementation, in any language" \
"There are two: Rust (reference) and Python (\`reference/python/\`, format
only). Both were written in this repository, so neither is an independent
reading of the specification.

A third — Go, TypeScript, Zig, anything — written by someone who has not read
the Rust is the single most useful contribution this project can receive. It
does not need to resolve or dispatch: parser, manifest validator and MCF-1 are
enough to run the conformance suite.

**Where to start:** \`spec/SPECIFICATION.md\` sections 3, 5 and 7.1, then
\`spec/conformance/README.md\`.

**What we want back most:** every place the spec was ambiguous. Those are
bugs in the document, and reporting them is worth more than the code." \
"help wanted,spec"

issue "Audit: dialog backends against real zenity/kdialog/osascript" \
"\`crates/murl-daemon/src/dialog_ui.rs\` is tested against stub programs that
emit the output the real backends are documented to emit. Nobody has run it
against the actual tools on a real desktop.

**Done when:** each backend is confirmed on a real session — the checklist
renders, nothing is pre-checked, Cancel grants nothing, and the returned ids
map back correctly. Report flag differences per version; the argv builders are
one function each.

**Security-relevant:** if any backend can be made to return an id it was not
offered, that is a finding, not a formatting bug." \
"help wanted,security,platform"

echo
echo "Done. Review them, and delete any that do not match where the project is."
