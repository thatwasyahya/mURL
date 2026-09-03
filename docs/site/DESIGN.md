# Site design brief

Read this before touching any page. The design system lives in `style.css`
and `site.js`; pages use it, they do not redefine it.

## Identity

**Technical editorial.** A serious engineering document that happens to be
beautiful — not a SaaS landing page. Dark canvas by default with a real
light theme. Monospace is part of the identity: `murl://` is the logo, mURLs
are always set in mono. One accent (cyan). Three tier colours — green
SAFE, amber SENSITIVE, red DANGEROUS — that mean something and are never
used as decoration.

The one flourish: **Instrument Serif italic** (`class="display"`) for a
single emphasised word inside a headline. Once per page, at most twice.
Example: `One address. <span class="display">Every</span> resource.`

Things that read as generic and must not appear: gradient text, purple/blue
gradient blobs, glassmorphism cards, three-identical-icon-cards rows as the
main content, emoji as icons, "Get started" buttons with no object, stock
hero illustrations, lorem-like copy. If a section could belong to any
product, rewrite it so it could only belong to this one.

## Every page has

1. This inline snippet in `<head>` **before** the stylesheet, so the theme
   applies before first paint (no flash) and screenshots can force a theme:

```html
<script>
(function(){try{var q=new URLSearchParams(location.search).get("theme");var t=q||localStorage.getItem("murl-theme");if(t)document.documentElement.setAttribute("data-theme",t);}catch(e){}})();
</script>
<link rel="stylesheet" href="style.css">
```

2. The shared header, exactly this shape (set `aria-current="page"` on the
   current link):

```html
<a class="skip" href="#main">Skip to content</a>
<header class="site-head">
  <div class="wrap">
    <a class="brand" href="index.html"><span class="mark">m</span><span class="scheme">murl</span>://</a>
    <nav id="nav">
      <a href="index.html">Overview</a>
      <a href="playground.html">Playground</a>
      <a href="spec.html">Specification</a>
      <a href="security.html">Security</a>
      <a href="https://github.com/thatwasyahya/mURL">GitHub <svg class="icon"><use href="vendor/icons.svg#i-arrow-up-right"/></svg></a>
    </nav>
    <div class="actions">
      <button class="icon-btn" type="button" data-theme-toggle aria-label="Toggle theme">
        <svg class="icon icon-sun"><use href="vendor/icons.svg#i-sun"/></svg>
        <svg class="icon icon-moon"><use href="vendor/icons.svg#i-moon"/></svg>
      </button>
      <button class="icon-btn nav-toggle" type="button" data-nav-toggle aria-label="Menu" aria-expanded="false" aria-controls="nav">
        <svg class="icon"><use href="vendor/icons.svg#i-menu"/></svg>
      </button>
    </div>
  </div>
</header>
```

3. The shared footer:

```html
<footer class="site-foot">
  <div class="wrap">
    <a class="brand" href="index.html"><span class="scheme">murl</span>://</a>
    <p>Experimental. Not a standard. MIT OR Apache-2.0 ·
      <a href="https://github.com/thatwasyahya/mURL">Source</a> ·
      <a href="https://github.com/thatwasyahya/mURL/issues">Issues</a> ·
      <a href="spec.html">Specification</a></p>
  </div>
</footer>
```

4. Scripts at the end of `<body>`, in this order:

```html
<script src="vendor/highlight.min.js"></script>
<script src="vendor/motion.js"></script>
<script src="site.js"></script>
<!-- page-specific script last -->
```

## Vocabulary (from style.css)

- Layout: `.wrap` (74rem), `.wrap-narrow` (46rem prose), `section`, `.split`
  (two columns on desktop), `.grid.cols-2/3/4`, `.stack`, `.row`, `.dots`
  (dot-grid background).
- Type: `.eyebrow` (mono uppercase label with a rule), `.lede`, `.display`,
  `.small`, `.muted`, `.faint`, `.mono`, `.tnum`.
- Buttons: `.btn`, `.btn-primary`, `.btn-ghost`, `.btn-sm`, `.btn-mono`,
  `.icon-btn`.
- Badges: `.badge`, `.badge.warn`, `.badge.accent`; tier pills
  `.pill.safe/.sensitive/.dangerous`, `.pill.kind`.
- Cards: `.card`, `.card.lift`, `.card.tier.tier-safe` etc., `.note`,
  `.note.warn`, `.linklist`.
- Code: wrap in `<div class="codeblock"><span class="lang">json</span>
  <button class="btn btn-sm btn-ghost copy" data-copy="ID">copy</button>
  <pre><code id="ID" class="language-json">…</code></pre></div>`.
  highlight.js colours it; languages available: json, bash, rust, yaml, ini,
  plaintext (use `language-plaintext` for terminal output you colour by hand).
- Terminal: `<div class="terminal"><div class="bar"><i></i><i></i><i></i>
  <span class="title">murl</span></div><pre>…</pre></div>` with spans
  `.prompt .cmd .dim .safe .sensitive .danger .cursor`.
- Tabs: `<div class="tabs" role="tablist" data-tabs="NAME"><button role="tab"
  data-tab="a">A</button>…</div>` and panels `<div data-panel="a"
  data-group="NAME">…</div>` (site.js wires them and remembers the choice).
- Forms: `.field > label + .input` / `textarea.input`.
- Stats: `.stats > .stat > .n + .l`.
- Icons: `<svg class="icon"><use href="vendor/icons.svg#i-NAME"/></svg>`.
  Available: arrow-right arrow-up-right check copy download github terminal
  folder file globe link-2 shield shield-alert shield-check key-round
  file-json layers sun moon menu x play book-open lock external-link zap
  circle-alert sparkles git-branch mail map-pin monitor server search
  chevron-right chevron-down.
- Reveal on scroll: add `data-reveal` (optional delay in seconds:
  `data-reveal="0.1"`). Motion animates it; without JS it is just visible.
- mURL strings: `<span class="murl-str"><span class="s">murl://</span><span
  class="a">acme.example/</span><span class="n">project-x</span><span
  class="f">#docs</span></span>`.
- Docs layout: `<div class="wrap"><div class="docs"><aside class="toc">…
  </aside><article class="prose">…</article></div></div>`. TOC links to h2
  ids; site.js highlights the active one and adds # anchors.

Page-specific CSS goes in a `<style>` block in that page's `<head>` and uses
tokens (`var(--bg-1)` etc.) — never raw colours except inside `.terminal`,
which is intentionally always dark. Do not edit `style.css`; if something
is missing, note it in your report.

## Copy

Keep the existing content's substance and its honesty: experimental, not a
standard, an mURL is a name and never a container, three tiers decided
locally, no shell ever. Facts to keep current: release **v0.5.0**, format
**v0.2**, 222 tests, 153 conformance vectors, two implementations (Rust
reference, Python), installs via `cargo install murl-cli`, `brew install
thatwasyahya/murl/murl`, `scoop bucket add murl
https://github.com/thatwasyahya/scoop-murl && scoop install murl`, winget
pending review.

## Verify what you ship

Screenshots are the acceptance test. From PowerShell:

```
powershell -ExecutionPolicy Bypass -File tools\site\shoot.ps1 index.html v1
powershell -ExecutionPolicy Bypass -File tools\site\shoot.ps1 index.html v1 light
```

Then look at the PNGs in `tools/site/shots/` (git-ignored). Look at desktop, the full-page
capture, mobile, and light theme. Fix what looks wrong and shoot again.
Mobile shots go through `dev-frame.html` (a true 390px iframe: Chrome will not
open a narrower window, so a plain 390px screenshot is a 500px layout cropped).
`dev-overflow.html?page=X` lists every element wider than the viewport.
Both are stripped from the deploy. Also run `python3 docs/site/check-wiring.py`
for the playground and confirm
the pages.yml link/tag check would pass (every relative href/src resolves,
tags balance).
