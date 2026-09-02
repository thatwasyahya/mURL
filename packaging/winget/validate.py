"""Validate the winget manifests against Microsoft's published schemas.

`winget validate` is a Windows-only tool, but the schemas it enforces are
public JSON Schema documents. Validating against them is the difference
between submitting something checked and submitting something hoped for.
"""
import io
import json
import os
import sys
import urllib.request

import jsonschema
import yaml

ROOT = os.path.dirname(os.path.abspath(__file__)) + "/"
VERSION = "1.12.0"
BASE = f"https://raw.githubusercontent.com/microsoft/winget-cli/master/schemas/JSON/manifests/v{VERSION}/"

FILES = {
    "thatwasyahya.murl.yaml": f"manifest.version.{VERSION}.json",
    "thatwasyahya.murl.installer.yaml": f"manifest.installer.{VERSION}.json",
    "thatwasyahya.murl.locale.en-US.yaml": f"manifest.defaultLocale.{VERSION}.json",
}

failures = 0
for filename, schema_name in FILES.items():
    url = BASE + schema_name
    try:
        with urllib.request.urlopen(url, timeout=60) as fh:
            schema = json.load(fh)
    except Exception as exc:  # noqa: BLE001
        print(f"[skip] {filename}: cannot fetch schema ({exc})")
        failures += 1
        continue

    doc = yaml.safe_load(io.open(ROOT + filename, encoding="utf-8").read())
    validator = jsonschema.Draft7Validator(schema)
    errors = sorted(validator.iter_errors(doc), key=lambda e: list(e.path))
    if errors:
        failures += 1
        print(f"[FAIL] {filename}: {len(errors)} schema error(s)")
        for e in errors[:6]:
            where = "/".join(str(x) for x in e.path) or "(root)"
            print(f"    {where}: {e.message[:160]}")
    else:
        print(f"[ok]   {filename}")

# Cross-file consistency: winget requires identifier and version to match.
docs = {
    name: yaml.safe_load(io.open(ROOT + name, encoding="utf-8").read())
    for name in FILES
}
ids = {d["PackageIdentifier"] for d in docs.values()}
versions = {d["PackageVersion"] for d in docs.values()}
mv = {d["ManifestVersion"] for d in docs.values()}
print(f"\nidentifiers: {ids}\nversions: {versions}\nmanifestVersions: {mv}")
if len(ids) != 1 or len(versions) != 1 or len(mv) != 1:
    print("FAIL: the three files disagree")
    failures += 1

types = sorted(d["ManifestType"] for d in docs.values())
print("manifest types:", types)
if types != ["defaultLocale", "installer", "version"]:
    print("FAIL: a submission needs exactly version + installer + defaultLocale")
    failures += 1

sys.exit(1 if failures else 0)
