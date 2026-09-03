// Shared behaviour for every page. Loaded after vendor/highlight.min.js and
// vendor/motion.js, both optional: everything here degrades to static markup
// when a library is missing or the visitor prefers reduced motion.
(function () {
  "use strict";
  const root = document.documentElement;
  const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  // ---- theme ---------------------------------------------------------
  // The initial theme is applied by an inline snippet in <head> (see
  // DESIGN.md) so the page does not flash. This only handles the toggle.
  function setTheme(next) {
    if (next) root.setAttribute("data-theme", next);
    else root.removeAttribute("data-theme");
    try { next ? localStorage.setItem("murl-theme", next) : localStorage.removeItem("murl-theme"); } catch (_) {}
  }
  function currentTheme() {
    const explicit = root.getAttribute("data-theme");
    if (explicit) return explicit;
    return window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
  }
  document.querySelectorAll("[data-theme-toggle]").forEach((btn) => {
    btn.addEventListener("click", () => setTheme(currentTheme() === "light" ? "dark" : "light"));
  });

  // ---- mobile nav ----------------------------------------------------
  document.querySelectorAll("[data-nav-toggle]").forEach((btn) => {
    const head = btn.closest(".site-head");
    btn.addEventListener("click", () => {
      const open = head.classList.toggle("open");
      btn.setAttribute("aria-expanded", String(open));
    });
  });

  // ---- syntax highlighting -------------------------------------------
  if (window.hljs) {
    document.querySelectorAll("pre code[class*='language-']").forEach((el) => {
      if (!el.dataset.highlighted) window.hljs.highlightElement(el);
    });
  }

  // ---- tabs ----------------------------------------------------------
  document.querySelectorAll("[data-tabs]").forEach((group) => {
    const name = group.dataset.tabs;
    const buttons = group.querySelectorAll("[data-tab]");
    const panels = document.querySelectorAll(`[data-panel][data-group="${name}"]`);
    function select(id) {
      buttons.forEach((b) => b.setAttribute("aria-selected", String(b.dataset.tab === id)));
      panels.forEach((p) => p.classList.toggle("active", p.dataset.panel === id));
      try { localStorage.setItem("murl-tab-" + name, id); } catch (_) {}
    }
    buttons.forEach((b) => b.addEventListener("click", () => select(b.dataset.tab)));
    let initial = null;
    try { initial = localStorage.getItem("murl-tab-" + name); } catch (_) {}
    const valid = Array.from(buttons).some((b) => b.dataset.tab === initial);
    select(valid ? initial : buttons[0] && buttons[0].dataset.tab);
  });

  // ---- copy buttons + toast ------------------------------------------
  let toast = document.querySelector(".toast");
  if (!toast) {
    toast = document.createElement("div");
    toast.className = "toast";
    toast.setAttribute("role", "status");
    document.body.appendChild(toast);
  }
  let toastTimer = null;
  window.murlToast = function (text) {
    toast.textContent = text;
    toast.classList.add("show");
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => toast.classList.remove("show"), 1600);
  };
  document.querySelectorAll("[data-copy]").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const target = document.getElementById(btn.dataset.copy);
      const text = target ? (target.value !== undefined && target.tagName === "TEXTAREA" ? target.value : target.textContent) : "";
      if (!text) return;
      try {
        await navigator.clipboard.writeText(text.trim());
        window.murlToast("Copied");
      } catch (_) {
        window.murlToast("Select and copy manually");
      }
    });
  });

  // ---- scroll reveals (Motion) ---------------------------------------
  const reveals = document.querySelectorAll("[data-reveal]");
  if (reduced || !window.Motion || !window.Motion.inView) {
    root.classList.add("no-motion");
  } else {
    reveals.forEach((el) => {
      const delay = parseFloat(el.dataset.reveal) || 0;
      window.Motion.inView(el, () => {
        window.Motion.animate(
          el,
          { opacity: [0, 1], transform: ["translateY(14px)", "translateY(0px)"] },
          { duration: 0.6, delay, ease: [0.2, 0.7, 0.2, 1] }
        );
        el.classList.add("in");
      }, { margin: "0px 0px -10% 0px" });
    });
  }

  // ---- table of contents highlighting --------------------------------
  const toc = document.querySelector(".toc");
  if (toc && "IntersectionObserver" in window) {
    const links = Array.from(toc.querySelectorAll("a[href^='#']"));
    const targets = links.map((a) => document.getElementById(a.getAttribute("href").slice(1))).filter(Boolean);
    const io = new IntersectionObserver((entries) => {
      entries.forEach((e) => {
        if (!e.isIntersecting) return;
        links.forEach((a) => a.classList.toggle("active", a.getAttribute("href") === "#" + e.target.id));
      });
    }, { rootMargin: "-20% 0px -70% 0px" });
    targets.forEach((t) => io.observe(t));
  }

  // ---- heading anchors on docs pages ---------------------------------
  document.querySelectorAll(".prose h2[id], .prose h3[id]").forEach((h) => {
    if (h.querySelector(".anchor")) return;
    const a = document.createElement("a");
    a.className = "anchor";
    a.href = "#" + h.id;
    a.setAttribute("aria-label", "Link to this section");
    a.textContent = "#";
    h.appendChild(a);
  });
})();
