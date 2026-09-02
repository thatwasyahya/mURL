#!/usr/bin/env python3
"""Render the README's terminal demo from real command output.

Run:  python3 docs/assets/make-demo.py

The point of generating it rather than hand-drawing it: the image shows what
the tool actually prints, and it can be regenerated when the output changes.
A screenshot nobody can reproduce goes stale silently, which for a project
whose selling point is "inspect before you act" would be a poor look.

No dependencies; writes docs/assets/demo.svg.
"""
import html
import os
import shutil
import subprocess
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
OUT = os.path.join(HERE, "demo.svg")

# Palette: the same greens/ambers/reds the docs and the site use, so the
# image belongs to the project rather than to a terminal theme.
BG = "#10241d"
FG = "#d8e6df"
DIM = "#7f948c"
ACCENT = "#6fd3af"
AMBER = "#d9a03f"
RED = "#e06b5e"
PROMPT = "#5fc9a2"

FONT = ("ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, "
        "'DejaVu Sans Mono', monospace")
CHAR_W = 8.4
LINE_H = 20
PAD = 22
TOP = 52


def capture():
    """Run the real commands in a throwaway state directory."""
    murl = os.path.join(ROOT, "target", "debug", "murl")
    if not os.path.exists(murl):
        raise SystemExit("build first: cargo build")
    state = tempfile.mkdtemp(prefix="murl-demo-")
    env = dict(os.environ,
               MURL_CONFIG_DIR=os.path.join(state, "config"),
               MURL_DATA_DIR=os.path.join(state, "data"),
               MURL_CACHE_DIR=os.path.join(state, "cache"))
    try:
        for name, path in (("demo/team", "examples/team.murl.json"),
                           ("demo/project-x", "examples/project-x.murl.json")):
            subprocess.run([murl, "name", "add", name, os.path.join(ROOT, path)],
                           env=env, cwd=ROOT, check=True, capture_output=True)
        out = subprocess.run([murl, "resolve", "murl://local/demo/project-x"],
                             env=env, cwd=ROOT, check=True,
                             capture_output=True, text=True).stdout
    finally:
        shutil.rmtree(state, ignore_errors=True)
    # The store path is a temp directory; show where it would really live.
    return out.replace(os.path.join(state, "data", "names"),
                       "~/.local/share/murl/names").rstrip("\n").split("\n")


def colour(line):
    """Colour a line by what it says, not by ANSI codes we did not emit."""
    if "DANGEROUS" in line:
        return RED
    if "SENSITIVE" in line:
        return AMBER
    if "consent:" in line:
        return DIM
    if line.startswith("  manifest:") or line.startswith("  Everything"):
        return DIM
    if "SAFE" in line:
        return FG
    return FG


def main():
    body = capture()
    command = "$ murl resolve murl://local/demo/project-x"
    lines = [command, ""] + body

    width = int(max(len(l) for l in lines) * CHAR_W) + PAD * 2
    height = TOP + len(lines) * LINE_H + PAD

    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" '
        f'height="{height}" viewBox="0 0 {width} {height}" '
        f'font-family="{FONT}" font-size="13">',
        f'<rect width="{width}" height="{height}" rx="10" fill="{BG}"/>',
        # Window chrome, so it reads as a terminal at a glance.
        f'<circle cx="{PAD}" cy="24" r="6" fill="#e06b5e"/>',
        f'<circle cx="{PAD + 20}" cy="24" r="6" fill="#d9a03f"/>',
        f'<circle cx="{PAD + 40}" cy="24" r="6" fill="#6fd3af"/>',
        f'<text x="{width // 2}" y="28" fill="{DIM}" font-size="11" '
        f'text-anchor="middle">mURL</text>',
    ]

    for i, line in enumerate(lines):
        y = TOP + i * LINE_H
        if not line:
            continue
        if line.startswith("$ "):
            parts.append(
                f'<text x="{PAD}" y="{y}" xml:space="preserve">'
                f'<tspan fill="{PROMPT}">$ </tspan>'
                f'<tspan fill="{FG}">{html.escape(line[2:])}</tspan></text>'
            )
        else:
            parts.append(
                f'<text x="{PAD}" y="{y}" fill="{colour(line)}" '
                f'xml:space="preserve">{html.escape(line)}</text>'
            )

    parts.append("</svg>")
    with open(OUT, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("\n".join(parts) + "\n")
    print(f"wrote {OUT} ({len(lines)} lines, {width}x{height})")


if __name__ == "__main__":
    main()
