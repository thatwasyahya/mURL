#!/usr/bin/env python3
"""Check the descriptive JSON Schema against the corpus it describes.

The schema is a tooling aid, not the normative validator (see its own
$comment). This script keeps it honest anyway: it must accept every valid
example and conformance vector, and it must reject the invalid vectors it
is *capable* of rejecting — the rules JSON Schema cannot express (duplicate
members, dependsOn integrity and acyclicity, notBefore < expires, per-kind
target syntax, integers inside free-form meta) are listed as expected
misses rather than silently ignored.

Usage: python3 spec/check-schema.py   (needs `jsonschema`)
Exit code 0 on success, 1 on any unexpected result.
"""
import json
import pathlib
import sys

try:
    import jsonschema
except ImportError:  # pragma: no cover - CI installs it
    print("this check needs the `jsonschema` package: pip install jsonschema")
    sys.exit(1)

ROOT = pathlib.Path(__file__).resolve().parent.parent
SCHEMA_PATH = ROOT / "spec" / "murl-manifest.schema.json"
CONFORMANCE = ROOT / "spec" / "conformance" / "manifests"
EXAMPLES = ROOT / "examples"

# Invalid vectors the schema legitimately cannot catch. Each entry names the
# rule that lives in the reference validator instead. Keeping this list
# explicit means a vector that starts passing for the *wrong* reason still
# shows up as a change to review.
SCHEMA_CANNOT_CATCH = {
    "duplicate-top-level-member.murl.json": "duplicate members (parser-level)",
    "duplicate-resource-member.murl.json": "duplicate members (parser-level)",
    "float-in-meta.murl.json": "meta is free-form; integers enforced by the validator",
    "duplicate-resource-ids.murl.json": "uniqueness across array items",
    "depends-on-unknown.murl.json": "referential integrity",
    "depends-on-self.murl.json": "referential integrity",
    "depends-on-cycle.murl.json": "acyclicity",
    "relation-unknown-id.murl.json": "referential integrity",
    "not-before-after-expires.murl.json": "cross-field ordering",
    "http-non-loopback.murl.json": "per-kind target syntax",
    "userinfo-in-target.murl.json": "per-kind target syntax",
    "relative-path-target.murl.json": "per-kind target syntax",
    "path-traversal-target.murl.json": "per-kind target syntax",
    "nested-murl-bad-target.murl.json": "per-kind target syntax",
    "control-char-in-label.murl.json": "control characters in strings",
    "id-with-selector.murl.json": "mURL grammar inside a string field",
    "ssh-option-smuggling.murl.json": "per-kind target syntax",
    "ssh-double-at.murl.json": "per-kind target syntax",
    "ssh-bad-scheme.murl.json": "per-kind target syntax",
    "remote-desktop-userinfo.murl.json": "per-kind target syntax",
    "geo-out-of-range.murl.json": "per-kind target syntax",
    "geo-missing-longitude.murl.json": "per-kind target syntax",
    "mailto-forbidden-header.murl.json": "per-kind target syntax",
    "mailto-not-an-address.murl.json": "per-kind target syntax",
}


def main() -> int:
    schema = json.loads(SCHEMA_PATH.read_text(encoding="utf-8"))
    jsonschema.Draft202012Validator.check_schema(schema)
    validator = jsonschema.Draft202012Validator(schema)
    failures = []

    def check_valid(path: pathlib.Path) -> None:
        doc = json.loads(path.read_text(encoding="utf-8"))
        errors = sorted(validator.iter_errors(doc), key=lambda e: list(e.path))
        if errors:
            failures.append(
                f"{path.name}: schema rejected a VALID document — "
                f"{list(errors[0].path)} {errors[0].message}"
            )

    for path in sorted(EXAMPLES.glob("*.murl.json")):
        check_valid(path)
    valid_dir = CONFORMANCE / "valid"
    for path in sorted(valid_dir.glob("*.murl.json")):
        check_valid(path)

    caught = 0
    for path in sorted((CONFORMANCE / "invalid").glob("*.murl.json")):
        try:
            doc = json.loads(path.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            # Duplicate-member vectors are still parseable by Python (last
            # wins); a genuine syntax error would be a broken vector.
            failures.append(f"{path.name}: vector is not parseable JSON")
            continue
        rejected = not validator.is_valid(doc)
        expected_miss = path.name in SCHEMA_CANNOT_CATCH
        if rejected:
            caught += 1
            if expected_miss:
                print(
                    f"note: {path.name} is now caught by the schema "
                    f"(listed as: {SCHEMA_CANNOT_CATCH[path.name]}) — "
                    "consider removing it from SCHEMA_CANNOT_CATCH"
                )
        elif not expected_miss:
            failures.append(
                f"{path.name}: schema accepted an INVALID document and it is "
                "not listed in SCHEMA_CANNOT_CATCH"
            )

    total_invalid = len(list((CONFORMANCE / "invalid").glob("*.murl.json")))
    print(
        f"schema check: {len(list(valid_dir.glob('*.murl.json')))} valid vectors + "
        f"{len(list(EXAMPLES.glob('*.murl.json')))} examples accepted; "
        f"{caught}/{total_invalid} invalid vectors caught by the schema alone "
        f"(the rest are the reference validator's job)"
    )
    for failure in failures:
        print(f"FAIL {failure}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
