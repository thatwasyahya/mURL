// Prove the wasm module behaves like the reference implementation before
// any UI is built on top of it. Run: node wasm_smoke.mjs <path-to-wasm>
import { readFileSync } from "node:fs";

const wasmPath = process.argv[2];
const { instance } = await WebAssembly.instantiate(readFileSync(wasmPath), {});
const { memory, murl_alloc, murl_free, murl_process } = instance.exports;

function call(request) {
  const bytes = new TextEncoder().encode(JSON.stringify(request));
  const ptr = murl_alloc(bytes.length);
  new Uint8Array(memory.buffer, ptr, bytes.length).set(bytes);
  const outPtr = murl_process(ptr, bytes.length);
  murl_free(ptr, bytes.length);

  const len = new DataView(memory.buffer).getUint32(outPtr, true);
  const json = new TextDecoder().decode(
    new Uint8Array(memory.buffer, outPtr + 4, len)
  );
  murl_free(outPtr, 4 + len);
  return JSON.parse(json);
}

let failures = 0;
function check(label, actual, expected) {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  if (a === e) {
    console.log(`  ok    ${label}`);
  } else {
    console.log(`  FAIL  ${label}\n        expected ${e}\n        actual   ${a}`);
    failures++;
  }
}

console.log("wasm smoke test");

const out = call({
  name: "Project X",
  slug: "project-x",
  authority: "acme.example",
  lines: [
    "https://github.com/acme/project-x",
    "~/projects/project-x",
    "terminal ~/projects/project-x",
    "~/notes/plan.md",
    "~/tools/setup.sh",
    "http://example.com/insecure",
    "../etc/passwd",
  ],
});

check("kinds", out.resources.map((r) => r.kind), [
  "https", "dir", "terminal", "file", "file", "", "",
]);
check("tiers", out.resources.map((r) => r.tier), [
  "SAFE", "SENSITIVE", "DANGEROUS", "SENSITIVE", "DANGEROUS", "", "",
]);
check(
  "bad lines carry an error",
  out.resources.slice(5).map((r) => r.error !== null),
  [true, true]
);
check("local identity", out.murl, "murl://local/project-x");
check("published identity", out.published.murl, "murl://acme.example/project-x");
check("no validation errors", out.errors, []);

// The manifest it emits must itself be valid, which the module checked with
// murl-core before returning it.
const manifest = JSON.parse(out.manifest);
check("manifest version", manifest.murlVersion, "0.2");
check("manifest id", manifest.id, "murl://local/project-x");
check("resource count", manifest.resources.length, 5);

// Empty input must be refused rather than producing an empty manifest.
check("empty input refused", call({ lines: [] }).ok, false);

// Memory must not run away: repeated calls should stay stable.
const before = memory.buffer.byteLength;
for (let i = 0; i < 500; i++) call({ slug: "x", lines: ["https://e.example/" + i] });
const grew = (memory.buffer.byteLength - before) / 1024 / 1024;
check("memory stable over 500 calls (<8MB growth)", grew < 8, true);

console.log(failures === 0 ? "\nPASS" : `\nFAIL: ${failures}`);
process.exit(failures === 0 ? 0 : 1);
