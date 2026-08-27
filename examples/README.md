# Examples

* **`demo.sh`** — the guided end-to-end tour: parse → validate → install →
  resolve (with nested splice) → selector → dry-run open → sign → verify →
  JSON output. Hermetic: all state lives in a temp directory and the only
  `open` is `--dry-run`, so nothing launches and nothing on your machine
  changes. Run it with `bash examples/demo.sh`.

* **`project-x.murl.json`** — the flagship destination: four web resources
  with roles and ordering, a workspace directory, a terminal that
  `dependsOn` the workspace, a nested team destination (`kind: murl`), and
  typed `relations` metadata.

* **`team.murl.json`** — the nested destination Project X composes,
  demonstrating recursive resolution and splicing.

The longer narrative, including publishing these remotely via
`/.well-known/murl/` and the trust flow, is in
[docs/examples.md](../docs/examples.md).
