"""Download the Lucide icons the site uses and pack them into one sprite.

Referenced as <svg class="icon"><use href="vendor/icons.svg#i-copy"/></svg>.
Stroke styling is left to CSS so the icons take the current text colour.
"""
import os
import re
import sys
import urllib.request

VERSION = "0.544.0"
ICONS = """
arrow-right arrow-up-right check copy download github terminal folder file
globe link-2 shield shield-alert shield-check key-round file-json layers sun
moon menu x play book-open lock external-link zap circle-alert sparkles
git-branch mail map-pin monitor server search chevron-right chevron-down
""".split()

here = os.path.dirname(os.path.abspath(__file__))
symbols = []
for name in ICONS:
    url = f"https://cdn.jsdelivr.net/npm/lucide-static@{VERSION}/icons/{name}.svg"
    try:
        with urllib.request.urlopen(url, timeout=30) as r:
            svg = r.read().decode("utf-8")
    except Exception as e:  # noqa: BLE001
        print(f"FAILED {name}: {e}", file=sys.stderr)
        sys.exit(1)
    inner = re.search(r"<svg[^>]*>(.*)</svg>", svg, re.S).group(1).strip()
    view = re.search(r'viewBox="([^"]+)"', svg).group(1)
    symbols.append(f'  <symbol id="i-{name}" viewBox="{view}">{inner}</symbol>')

sprite = (
    '<svg xmlns="http://www.w3.org/2000/svg" style="display:none">\n'
    + "\n".join(symbols)
    + "\n</svg>\n"
)
out = os.path.join(here, "icons.svg")
with open(out, "w", encoding="utf-8") as f:
    f.write(sprite)
print(f"icons.svg: {len(symbols)} symbols, {os.path.getsize(out)} bytes (lucide {VERSION})")
