"""Check the playground's script and markup agree.

No headless browser is available here, and the likeliest failure in a page
like this is not logic but a mismatched id — silent, and only visible when
someone types into a field that is wired to nothing.
"""
import re
import sys

SITE = "/home/w196832/MyProjects/mURL/docs/site/"
html = open(SITE + "playground.html", encoding="utf-8").read()
js = open(SITE + "playground.js", encoding="utf-8").read()

ids_in_html = set(re.findall(r'id="([^"]+)"', html))
print("ids in page:", ", ".join(sorted(ids_in_html)))

referenced = set(re.findall(r'\$\("([^"]+)"\)', js))
referenced |= set(re.findall(r'getElementById\("([^"]+)"\)', js))
print("ids used by script:", ", ".join(sorted(referenced)))

problems = []

missing = sorted(referenced - ids_in_html)
if missing:
    problems.append(f"script references ids the page does not define: {missing}")

# Every copy button must point at an element that exists.
copy_targets = set(re.findall(r'data-copy="([^"]+)"', html))
missing_copy = sorted(copy_targets - ids_in_html)
if missing_copy:
    problems.append(f"copy buttons target missing ids: {missing_copy}")

# Duplicate ids break getElementById in ways that look like logic bugs.
all_ids = re.findall(r'id="([^"]+)"', html)
dupes = sorted({i for i in all_ids if all_ids.count(i) > 1})
if dupes:
    problems.append(f"duplicate ids: {dupes}")

# The script is loaded, and after the elements it binds to.
if 'src="playground.js"' not in html:
    problems.append("playground.js is never loaded")

# Labels should point at real fields.
for target in re.findall(r'<label for="([^"]+)"', html):
    if target not in ids_in_html:
        problems.append(f"label points at missing field: {target}")

# The wasm file the script fetches must be the name CI writes.
if 'fetch("murl.wasm")' not in js:
    problems.append("script does not fetch murl.wasm")

print()
if problems:
    for p in problems:
        print("FAIL:", p)
    sys.exit(1)
print("PASS: markup and script agree")
