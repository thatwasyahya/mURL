# Examples

Everything below is runnable from a fresh clone. The one-command version:

```bash
bash examples/demo.sh    # hermetic: temp state, dry-run open, auto-cleanup
```

## The Project X walkthrough

`examples/project-x.murl.json` defines a development destination: four web
resources (GitHub, docs, Jira, Grafana), a local workspace directory, a
terminal in it (dependsOn: workspace), and a nested team destination
(`kind: murl` → `examples/team.murl.json`).

```bash
cargo build

# 1 · validate both manifests against the spec
./target/debug/murl validate examples/team.murl.json
./target/debug/murl validate examples/project-x.murl.json

# 2 · install them under local names
./target/debug/murl name add demo/team examples/team.murl.json
./target/debug/murl name add demo/project-x examples/project-x.murl.json

# 3 · resolve: the full plan, tiers, trust, nested splice
./target/debug/murl resolve murl://local/demo/project-x
```

```text
Project X  (murl://local/demo/project-x)
  Everything that makes up the Project X development context.
  manifest: local store (…/names/demo/project-x.murl.json)  trust: LOCAL
  manifest: local store (…/names/demo/team.murl.json)  trust: LOCAL

  resources:
    ├─ source      SAFE       https      https://github.com/example/project-x
    ├─ docs        SAFE       https      https://docs.example.com/project-x
    ├─ issues      SAFE       https      https://jira.example.com/projects/PX
    ├─ monitoring  SAFE       https      https://grafana.example.com/d/project-x
    ├─ workspace   SENSITIVE  dir        ~/projects/project-x
    ├─ term        DANGEROUS  terminal   ~/projects/project-x
    ├─ wiki        SAFE       https      https://wiki.example.com/team
    └─ chat        SAFE       https      https://chat.example.com/#/room/team
```

`wiki` and `chat` were spliced from the nested `demo/team` destination;
`term` is DANGEROUS and — because the manifest is LOCAL — promptable rather
than denied.

```bash
# 4 · address one part of the destination
./target/debug/murl resolve 'murl://local/demo/project-x#monitoring'

# 5 · open (interactive consent), or preview with --dry-run
./target/debug/murl open murl://local/demo/project-x --dry-run
./target/debug/murl open murl://local/demo/project-x            # prompts
./target/debug/murl open murl://local/demo/project-x --yes      # SAFE only
```

## Signing and trusting

```bash
./target/debug/murl keygen
./target/debug/murl sign examples/project-x.murl.json     # signs in place
./target/debug/murl verify examples/project-x.murl.json  # VALID + key id

# a consumer pins your key for your authority:
./target/debug/murl trust add acme.example <publicKey-or-key-file>
```

## Publishing a remote destination

A namespace is a static directory on any HTTPS server:

```text
https://acme.example/.well-known/murl/project-x.murl.json         @latest
https://acme.example/.well-known/murl/project-x@1.0.0.murl.json   pinned
```

Then anyone can run `murl open murl://acme.example/project-x`. Local
dry-run of the whole remote flow (loopback HTTP is permitted for
development):

```bash
mkdir -p /tmp/murl-www/.well-known/murl
cp examples/project-x.murl.json /tmp/murl-www/.well-known/murl/demo.murl.json
python3 -m http.server 8080 --directory /tmp/murl-www &
./target/debug/murl resolve murl://127.0.0.1:8080/demo
```

(Resolved remotely, the manifest shows `trust: UNSIGNED`, the workspace/
terminal resources gain "remote manifest references the local filesystem"
consent reasons, and `term` flips to **denied** — the trust gate in action.
Note the example's `id` is `murl://local/demo/project-x`, so this remote
serving is also refused until you adjust `id`: identity binding, spec §6.4,
demonstrating exactly the relabeling defense.)

## OS activation (Linux)

```bash
./target/debug/murl os install
xdg-open murl://local/demo/project-x     # or click one anywhere
./target/debug/murl os status
```

## Failure semantics, visibly

```bash
# a destination with a dead resource still opens the rest:
./target/debug/murl open murl://local/demo/project-x --yes
# → monitoring might be UNAVAILABLE; aggregate: PARTIAL_SUCCESS (exit 3)
```

Exit codes: 0 success · 1 failed · 2 invalid · 3 partial · 4 denied.
