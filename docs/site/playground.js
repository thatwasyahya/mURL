// The playground calls murl-core compiled to WebAssembly. Every kind,
// every tier and every validation message you see comes from the same code
// the CLI runs — not from a second copy of the rules written for a web page,
// which would drift from the specification the first time the spec moved.
//
// The ABI is three exports and a length-prefixed buffer; see
// crates/murl-wasm/src/lib.rs.
//
// site.js already wires the tabs, the data-copy buttons and the toast; this
// file only talks to the module and renders what comes back.

(function () {
  "use strict";

  const state = { wasm: null, ready: false, last: null, timer: null, preset: null };
  const $ = (id) => document.getElementById(id);
  const esc = (s) =>
    String(s).replace(/[&<>"']/g, (c) => (
      { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]
    ));

  // ---- examples ------------------------------------------------------
  const PRESETS = {
    frontend: {
      name: "Storefront web",
      slug: "storefront-web",
      description: "Everything that makes up the storefront frontend.",
      authority: "",
      lines: [
        "https://github.com/acme/storefront-web",
        "https://storefront.staging.acme.example",
        "https://design.acme.example/files/storefront",
        "https://ci.acme.example/pipelines/storefront-web",
        "~/code/storefront-web",
        "~/code/storefront-web/README.md",
        "terminal ~/code/storefront-web",
      ],
    },
    oncall: {
      name: "Payments on-call",
      slug: "payments-oncall",
      description: "Where to look, and what to open, when payments pages you.",
      authority: "acme.example",
      lines: [
        "https://grafana.acme.example/d/payments-overview",
        "https://alerts.acme.example/services/payments",
        "https://runbooks.acme.example/payments",
        "https://github.com/acme/payments-service",
        "http://wiki.acme.example/legacy-escalation",
        "~/oncall/payments",
        "~/oncall/payments/escalation.md",
        "terminal ~/oncall/payments",
      ],
    },
    paper: {
      name: "Attention paper, camera-ready",
      slug: "attention-paper",
      description: "The draft, its experiments, and the reading it builds on.",
      authority: "",
      lines: [
        "https://arxiv.org/abs/1706.03762",
        "https://github.com/acme-lab/attention-experiments",
        "https://tracking.acme.example/runs/attn-v3",
        "https://docs.acme.example/paper-notes",
        "~/research/attention",
        "~/research/attention/draft.tex",
        "~/research/attention/bib/refs.bib",
        "terminal ~/research/attention",
      ],
    },
  };

  // ---- wasm ----------------------------------------------------------
  async function boot() {
    const engine = $("engine");
    try {
      const response = await fetch("murl.wasm");
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      const bytes = await response.arrayBuffer();
      const { instance } = await WebAssembly.instantiate(bytes, {});
      state.wasm = instance.exports;
      state.ready = true;
      engine.textContent = `murl-core ${Math.round(bytes.byteLength / 1000)} KB, running locally`;
      engine.classList.remove("loading");
      engine.classList.add("ok");
      run();
    } catch (err) {
      engine.textContent = `validator unavailable (${err.message})`;
      engine.classList.remove("loading");
      engine.classList.add("bad");
      $("parsed").innerHTML =
        `<p class="hint">Could not load murl-core (${esc(err.message)}). ` +
        `The rules described below are still accurate, but this page cannot check your input.</p>`;
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

  // ---- run -----------------------------------------------------------
  function schedule() {
    clearTimeout(state.timer);
    state.timer = setTimeout(run, 120);
  }

  function run() {
    const lines = $("resources").value.split("\n");
    if (!state.ready) {
      renderGutter(lines, []);
      return;
    }
    const result = call({
      name: $("name").value,
      description: $("description").value,
      slug: $("slug").value,
      authority: $("authority").value,
      lines,
    });
    state.last = result;
    const resources = result.resources || [];

    renderGutter(lines, resources);
    renderGraph(result);
    renderResources(resources);
    renderProblems(result);
    renderManifest(result);
    renderUse(result);
  }

  // Map each returned resource back to the textarea line it came from, so
  // the gutter can colour the line. The module skips blank lines; match on
  // the trimmed text and fall back to "next non-blank line".
  function lineMap(lines, resources) {
    const map = new Array(lines.length).fill(null);
    let cursor = 0;
    resources.forEach((r) => {
      let hit = -1;
      for (let i = cursor; i < lines.length; i++) {
        if (lines[i].trim() === String(r.line || "").trim()) { hit = i; break; }
      }
      if (hit < 0) {
        for (let i = cursor; i < lines.length; i++) {
          if (lines[i].trim() !== "") { hit = i; break; }
        }
      }
      if (hit >= 0) { map[hit] = r; cursor = hit + 1; }
    });
    return map;
  }

  function renderGutter(lines, resources) {
    const map = lineMap(lines, resources);
    $("gutter").innerHTML = lines
      .map((_, i) => {
        const r = map[i];
        const cls = !r ? "" : r.error ? "bad" : String(r.tier || "").toLowerCase();
        return `<div class="${cls}">${i + 1}</div>`;
      })
      .join("");
    const ok = resources.filter((r) => !r.error).length;
    const bad = resources.length - ok;
    $("count").textContent = resources.length
      ? `${ok} accepted${bad ? ` · ${bad} rejected` : ""}`
      : "";
  }

  // ---- resolution graph ---------------------------------------------
  const TIER_COLOR = { SAFE: "var(--safe)", SENSITIVE: "var(--sensitive)", DANGEROUS: "var(--danger)" };
  const MAX_NODES = 14;

  function fit(text, px, fontSize) {
    const cw = fontSize * 0.6; // Plex Mono advance width
    const max = Math.max(3, Math.floor(px / cw));
    return text.length <= max ? text : text.slice(0, max - 1) + "…";
  }

  function renderGraph(result) {
    const svg = $("graph");
    const W = Math.max(280, svg.clientWidth || svg.parentElement.clientWidth - 20);
    const all = (result && result.resources) || [];
    const shown = all.slice(0, MAX_NODES);
    const extra = all.length - shown.length;
    const rows = Math.max(1, shown.length + (extra > 0 ? 1 : 0));
    const rowH = 26, padY = 14, fs = 11.5;
    const H = rows * rowH + padY * 2;
    const midY = H / 2;

    const slug = (result && result.slug) || "destination";
    const narrow = W < 460;
    const label = narrow ? `local/${slug}` : `murl://local/${slug}`;
    const leftMaxW = Math.floor(W * (narrow ? 0.38 : 0.42));
    const leftText = fit(label, leftMaxW - 22, fs);
    const leftW = Math.min(leftMaxW, Math.ceil(leftText.length * fs * 0.6) + 22);
    const leftX = 2, leftH = 30;
    const leftY = midY - leftH / 2;
    const fromX = leftX + leftW;

    const rightX = fromX + Math.max(48, Math.min(120, W * 0.14));
    const dotX = rightX + 6;
    const textX = dotX + 12;
    const textW = W - textX - 4;

    let out = "";
    // links first so nodes paint over them
    shown.forEach((r, i) => {
      const y = padY + i * rowH + rowH / 2;
      const c1 = fromX + (dotX - fromX) * 0.55, c2 = dotX - (dotX - fromX) * 0.45;
      const color = r.error ? "var(--danger)" : (TIER_COLOR[r.tier] || "var(--fg-2)");
      out += `<path d="M${fromX},${midY} C${c1},${midY} ${c2},${y} ${dotX - 4},${y}" fill="none" stroke="${color}" stroke-width="1.5" opacity="${r.error ? .6 : .85}"${r.error ? ' stroke-dasharray="3 4"' : ""}/>`;
    });
    if (shown.length === 0) {
      out += `<path d="M${fromX},${midY} L${dotX - 4},${midY}" fill="none" stroke="var(--line-strong)" stroke-width="1.5" stroke-dasharray="3 4"/>`;
      out += `<text x="${textX}" y="${midY + 4}" font-size="${fs}" fill="var(--fg-2)">${esc(fit("nothing to resolve yet", textW, fs))}</text>`;
    }
    // the mURL node
    out += `<rect x="${leftX}" y="${leftY}" width="${leftW}" height="${leftH}" rx="7" fill="var(--bg-1)" stroke="var(--accent-line)" stroke-width="1"/>`;
    out += `<text x="${leftX + 11}" y="${midY + 4}" font-size="${fs}" font-weight="600" fill="var(--fg-0)">` +
      (leftText.startsWith("murl://")
        ? `<tspan fill="var(--accent)">murl://</tspan>${esc(leftText.slice(7))}`
        : esc(leftText)) +
      `</text>`;
    // one node per resource
    shown.forEach((r, i) => {
      const y = padY + i * rowH + rowH / 2;
      if (r.error) {
        out += `<circle cx="${dotX}" cy="${y}" r="3.5" fill="none" stroke="var(--danger)" stroke-width="1.5"/>`;
        out += `<text x="${textX}" y="${y + 4}" font-size="${fs}" fill="var(--danger)" text-decoration="line-through" opacity=".9">${esc(fit(r.line, textW, fs))}</text>`;
        return;
      }
      const color = TIER_COLOR[r.tier] || "var(--fg-2)";
      out += `<circle cx="${dotX}" cy="${y}" r="4" fill="${color}"/>`;
      const id = String(r.id);
      const kind = String(r.kind);
      const idW = Math.ceil(id.length * fs * 0.6);
      const room = textW - idW - 10;
      const kindText = room > 3 * fs ? fit(kind, room, fs) : "";
      out += `<text x="${textX}" y="${y + 4}" font-size="${fs}"><tspan font-weight="600" fill="var(--fg-0)">${esc(fit(id, textW, fs))}</tspan>` +
        (kindText ? `<tspan dx="8" fill="var(--fg-2)">${esc(kindText)}</tspan>` : "") + `</text>`;
    });
    if (extra > 0) {
      const y = padY + shown.length * rowH + rowH / 2;
      out += `<text x="${textX}" y="${y + 4}" font-size="${fs}" fill="var(--fg-2)">+${extra} more</text>`;
    }

    svg.setAttribute("viewBox", `0 0 ${W} ${H}`);
    svg.setAttribute("height", String(H));
    const title = svg.querySelector("title");
    svg.innerHTML = "";
    if (title) svg.appendChild(title);
    svg.insertAdjacentHTML("beforeend", out);
  }

  // ---- table + problems ---------------------------------------------
  function renderResources(resources) {
    const el = $("parsed");
    if (resources.length === 0) {
      el.innerHTML = `<p class="hint">Paste a URL or a path on the left — one per line.</p>`;
      return;
    }
    const rows = resources
      .map((r) => {
        if (r.error) {
          return `<tr class="bad">
            <td class="line" colspan="2"><code>${esc(r.line)}</code></td>
            <td class="why"><span class="pill dangerous">rejected</span>${esc(r.error)}</td></tr>`;
        }
        const tier = String(r.tier).toLowerCase();
        return `<tr>
          <td class="id">${esc(r.id)}</td>
          <td class="target"><span class="pill kind">${esc(r.kind)}</span> ${esc(r.target)}</td>
          <td><span class="pill ${tier}">${esc(r.tier)}</span></td>
        </tr>`;
      })
      .join("");
    el.innerHTML = `<div class="table-scroll"><table>
        <thead><tr><th>id</th><th>resource</th><th>risk</th></tr></thead>
        <tbody>${rows}</tbody></table></div>`;
  }

  function renderProblems(result) {
    const el = $("problems");
    const errors = result.errors || [];
    const warnings = result.warnings || [];
    if (errors.length === 0 && warnings.length === 0) {
      el.innerHTML = "";
      return;
    }
    const item = (cls, icon, text) =>
      `<li class="${cls}"><svg class="icon"><use href="vendor/icons.svg#${icon}"/></svg><span>${esc(text)}</span></li>`;
    el.innerHTML = `<ul class="problems">
        ${errors.map((e) => item("err", "i-circle-alert", e)).join("")}
        ${warnings.map((w) => item("warn", "i-shield-alert", w)).join("")}
      </ul>`;
  }

  // ---- manifest + commands ------------------------------------------
  function setCode(id, text) {
    const code = $(id);
    code.innerText = text;
    delete code.dataset.highlighted;
    if (window.hljs && text) window.hljs.highlightElement(code);
  }

  function renderManifest(result) {
    const manifest = result.manifest || "";
    setCode("manifest", manifest || "// fix the errors on the Resources tab to get a manifest");
    $("download").hidden = !manifest;
    $("filename").textContent = result.filename || `${result.slug || "destination"}.murl.json`;
  }

  function murlStr(murl) {
    const m = /^murl:\/\/([^/]+\/)(.*)$/.exec(murl || "");
    if (!m) return `<span class="s">${esc(murl)}</span>`;
    return `<span class="s">murl://</span><span class="a">${esc(m[1])}</span><span class="n">${esc(m[2])}</span>`;
  }

  function renderUse(result) {
    const slug = result.slug || "destination";
    const file = result.filename || `${slug}.murl.json`;
    const murl = result.murl || `murl://local/${slug}`;

    $("local-murl").innerHTML = murlStr(murl);

    setCode("install",
      `murl name add ${slug} ${file}\n` +
      `murl resolve ${murl}   # see the plan; opens nothing\n` +
      `murl open ${murl}`);

    const pub = result.published;
    $("pub-box").hidden = !pub;
    if (pub) {
      $("pub-murl").innerHTML = murlStr(pub.murl);
      setCode("publish",
        `# serve the manifest at exactly this path on your domain:\n` +
        `#   ${pub.url}\n\n` +
        `# then anyone can run:\n` +
        `murl resolve ${pub.murl}\n` +
        `murl open ${pub.murl}`);
    } else {
      setCode("publish",
        `# fill in "publish under" on the left to see the hosting path:\n` +
        `#   https://<your-domain>/.well-known/murl/${file}`);
    }
  }

  function download() {
    const text = (state.last && state.last.manifest) || "";
    if (!text) return;
    const name = (state.last.filename || `${$("slug").value || "destination"}.murl.json`).replace(/[^a-z0-9._-]/gi, "-");
    const blob = new Blob([text], { type: "application/json" });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = name;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(a.href);
    if (window.murlToast) window.murlToast(`Saved ${name}`);
  }

  // ---- presets -------------------------------------------------------
  function loadPreset(key, quiet) {
    const p = PRESETS[key];
    if (!p) return;
    state.preset = key;
    $("name").value = p.name;
    $("slug").value = p.slug;
    $("description").value = p.description;
    $("authority").value = p.authority;
    $("resources").value = p.lines.join("\n");
    document.querySelectorAll("[data-preset]").forEach((b) =>
      b.setAttribute("aria-pressed", String(b.dataset.preset === key)));
    run();
    if (!quiet && window.murlToast) window.murlToast("Example loaded");
  }

  function clearPreset() {
    if (!state.preset) return;
    state.preset = null;
    document.querySelectorAll("[data-preset]").forEach((b) => b.setAttribute("aria-pressed", "false"));
  }

  // ---- wiring --------------------------------------------------------
  document.addEventListener("DOMContentLoaded", () => {
    ["name", "description", "slug", "authority", "resources"].forEach((id) =>
      $(id).addEventListener("input", () => { clearPreset(); schedule(); })
    );
    const ta = $("resources");
    ta.addEventListener("scroll", () => { $("gutter").scrollTop = ta.scrollTop; });
    $("download").addEventListener("click", download);
    document.querySelectorAll("[data-preset]").forEach((b) =>
      b.addEventListener("click", () => loadPreset(b.dataset.preset)));

    // the graph is sized to its container; redraw when that changes
    if ("ResizeObserver" in window) {
      let w = 0;
      new ResizeObserver(() => {
        const now = $("graph").clientWidth;
        if (now && now !== w) { w = now; if (state.last) renderGraph(state.last); }
      }).observe($("graph").parentElement);
    }
    // tabs are display:none until selected; redraw when the graph becomes visible
    document.querySelectorAll('[data-tabs="out"] [data-tab]').forEach((b) =>
      b.addEventListener("click", () => { if (state.last) requestAnimationFrame(() => renderGraph(state.last)); }));

    // ?tab=manifest deep-links a tab (site.js remembers the last one otherwise)
    try {
      const want = new URLSearchParams(location.search).get("tab");
      const btn = want && document.querySelector(`[data-tabs="out"] [data-tab="${want}"]`);
      if (btn) btn.click();
    } catch (_) {}

    loadPreset("frontend", true);
    boot();
  });
})();
