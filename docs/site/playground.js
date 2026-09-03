// The playground calls murl-core compiled to WebAssembly. Every kind,
// every tier and every validation message you see comes from the same code
// the CLI runs — not from a second copy of the rules written for a web page,
// which would drift from the specification the first time the spec moved.
//
// The ABI is three exports and a length-prefixed buffer; see
// crates/murl-wasm/src/lib.rs.

const state = { wasm: null, ready: false };

async function boot() {
  try {
    const response = await fetch("murl.wasm");
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const bytes = await response.arrayBuffer();
    const { instance } = await WebAssembly.instantiate(bytes, {});
    state.wasm = instance.exports;
    state.ready = true;
    document.getElementById("engine").textContent =
      `murl-core ${(bytes.byteLength / 1024).toFixed(0)} KB, running locally`;
    run();
  } catch (err) {
    document.getElementById("engine").textContent =
      `could not load the validator (${err.message}) — the examples below are still accurate, but this page cannot check your input`;
    document.getElementById("engine").classList.add("bad");
  }
}

function call(request) {
  const { memory, murl_alloc, murl_free, murl_process } = state.wasm;
  const bytes = new TextEncoder().encode(JSON.stringify(request));
  const ptr = murl_alloc(bytes.length);
  new Uint8Array(memory.buffer, ptr, bytes.length).set(bytes);
  const out = murl_process(ptr, bytes.length);
  murl_free(ptr, bytes.length);
  const len = new DataView(memory.buffer).getUint32(out, true);
  const text = new TextDecoder().decode(new Uint8Array(memory.buffer, out + 4, len));
  murl_free(out, 4 + len);
  return JSON.parse(text);
}

const $ = (id) => document.getElementById(id);
const esc = (s) =>
  String(s).replace(/[&<>"']/g, (c) => (
    { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]
  ));

function run() {
  if (!state.ready) return;

  const result = call({
    name: $("name").value,
    description: $("description").value,
    slug: $("slug").value,
    authority: $("authority").value,
    lines: $("resources").value.split("\n"),
  });

  renderResources(result.resources || []);
  renderProblems(result);
  renderOutput(result);
}

function renderResources(resources) {
  const el = $("parsed");
  if (resources.length === 0) {
    el.innerHTML = `<p class="hint">Paste a URL or a path above — one per line.</p>`;
    return;
  }
  const rows = resources
    .map((r) => {
      if (r.error) {
        return `<tr class="row-bad">
          <td colspan="2"><code>${esc(r.line)}</code></td>
          <td class="why">${esc(r.error)}</td></tr>`;
      }
      const tier = r.tier.toLowerCase();
      return `<tr>
        <td><code>${esc(r.id)}</code></td>
        <td><span class="kind">${esc(r.kind)}</span> <code class="target">${esc(r.target)}</code></td>
        <td><span class="tier ${tier}">${esc(r.tier)}</span></td>
      </tr>`;
    })
    .join("");
  el.innerHTML = `<table class="parsed">
      <thead><tr><th>id</th><th>resource</th><th>risk</th></tr></thead>
      <tbody>${rows}</tbody></table>`;
}

function renderProblems(result) {
  const el = $("problems");
  const errors = result.errors || [];
  const warnings = result.warnings || [];
  if (errors.length === 0 && warnings.length === 0) {
    el.innerHTML = "";
    return;
  }
  const item = (cls, text) => `<li class="${cls}">${esc(text)}</li>`;
  el.innerHTML = `<ul class="problems">
      ${errors.map((e) => item("err", e)).join("")}
      ${warnings.map((w) => item("warn", w)).join("")}
    </ul>`;
}

function renderOutput(result) {
  const manifest = result.manifest || "";
  $("manifest").textContent = manifest;
  $("download").classList.toggle("hidden", !manifest);

  const slug = result.slug || "destination";
  const file = result.filename || "destination.murl.json";

  $("install").textContent =
    `murl name add ${slug} ${file}\n` +
    `murl resolve ${result.murl || ""}      # see the plan, opens nothing\n` +
    `murl open ${result.murl || ""}`;

  const pub = result.published;
  $("publish").textContent = pub
    ? `# serve the manifest at this exact path on your own domain:\n` +
      `#   ${pub.url}\n\n` +
      `# then anyone can run:\n` +
      `murl open ${pub.murl}`
    : `# fill in "publish under" above to see the hosting path`;
}

function download() {
  const text = $("manifest").textContent;
  if (!text) return;
  const name = ($("slug").value || "destination").replace(/[^a-z0-9._-]/gi, "-");
  const blob = new Blob([text], { type: "application/json" });
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = `${name}.murl.json`;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(a.href);
}

async function copy(button, sourceId) {
  const text = $(sourceId).textContent;
  if (!text) return;
  try {
    await navigator.clipboard.writeText(text);
    const original = button.textContent;
    button.textContent = "copied";
    setTimeout(() => (button.textContent = original), 1200);
  } catch {
    button.textContent = "press ctrl+c";
  }
}

function loadExample() {
  $("name").value = "Project X";
  $("slug").value = "project-x";
  $("description").value = "Everything that makes up the Project X context.";
  $("authority").value = "";
  $("resources").value = [
    "https://github.com/example/project-x",
    "https://docs.example.com/project-x",
    "https://grafana.example.com/d/project-x",
    "~/projects/project-x",
    "terminal ~/projects/project-x",
    "~/projects/project-x/NOTES.md",
  ].join("\n");
  run();
}

document.addEventListener("DOMContentLoaded", () => {
  ["name", "description", "slug", "authority", "resources"].forEach((id) =>
    $(id).addEventListener("input", run)
  );
  $("download").addEventListener("click", download);
  $("example").addEventListener("click", loadExample);
  document.querySelectorAll("[data-copy]").forEach((b) =>
    b.addEventListener("click", () => copy(b, b.dataset.copy))
  );
  boot();
});
