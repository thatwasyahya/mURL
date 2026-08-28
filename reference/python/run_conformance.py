#!/usr/bin/env python3
"""Run spec/conformance/ against this implementation.

The rules are exactly the ones in ``spec/conformance/README.md``:

1. valid manifests parse and validate with zero errors (warnings are fine);
2. invalid manifests are rejected, at parse time or with >=1 validation error;
3. valid mURLs parse and round-trip through their canonical form;
4. invalid mURLs are rejected.

Each rule asserts a minimum vector count before running, so pointing this at
the wrong directory fails loudly instead of passing over an empty one.

Usage:  python3 run_conformance.py [--suite PATH] [-v]
Exit:   0 if every vector passes, 1 otherwise.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Callable, List, Tuple

sys.path.insert(0, str(Path(__file__).resolve().parent))

from murl_ref.canonical import canonical_bytes
from murl_ref.manifest import Manifest, ManifestError  # noqa: E402
from murl_ref.parser import MurlSyntaxError, parse_murl  # noqa: E402

DEFAULT_SUITE = Path(__file__).resolve().parents[2] / "spec" / "conformance"

Failure = Tuple[str, str]  # (vector name, why it failed)


class Rule:
    """One conformance rule and the failures it found."""

    def __init__(self, number: int, title: str, minimum: int) -> None:
        self.number = number
        self.title = title
        self.minimum = minimum
        self.checked = 0
        self.names: List[str] = []
        self.failures: List[Failure] = []

    def note(self, name: str) -> str:
        """Record that a vector was checked, and return its name."""
        self.checked += 1
        self.names.append(name)
        return name

    @property
    def passed(self) -> int:
        return self.checked - len(self.failures)

    @property
    def ok(self) -> bool:
        return not self.failures and self.checked >= self.minimum


def _read_manifests(suite: Path, sub: str) -> List[Tuple[str, bytes]]:
    directory = suite / "manifests" / sub
    if not directory.is_dir():
        raise SystemExit(f"conformance suite not found: {directory}")
    return [
        (path.name, path.read_bytes())
        for path in sorted(directory.iterdir())
        if path.name.endswith(".murl.json")
    ]


def _read_lines(suite: Path, name: str) -> List[str]:
    path = suite / "murls" / name
    if not path.is_file():
        raise SystemExit(f"conformance suite not found: {path}")
    # Trailing whitespace is stripped, but blank lines are kept: the empty
    # string is itself an mURL an implementation must reject, and dropping it
    # would silently shrink the suite.
    text = path.read_text(encoding="utf-8")
    lines = text.split("\n")
    if lines and lines[-1] == "":
        lines.pop()  # the file's final newline, not a vector
    return [line.rstrip() for line in lines]


def rule_valid_manifests(suite: Path) -> Rule:
    rule = Rule(1, "valid manifests parse and validate", minimum=10)
    for name, data in _read_manifests(suite, "valid"):
        rule.note(name)
        try:
            manifest = Manifest.from_bytes(data)
        except ManifestError as exc:
            rule.failures.append((name, f"failed to parse: {exc}"))
            continue
        report = manifest.validate()
        if not report.is_valid():
            rule.failures.append((name, "validation errors: " + "; ".join(report.errors)))
    return rule


def rule_invalid_manifests(suite: Path) -> Rule:
    rule = Rule(2, "invalid manifests are rejected", minimum=18)
    for name, data in _read_manifests(suite, "invalid"):
        rule.note(name)
        try:
            manifest = Manifest.from_bytes(data)
        except ManifestError:
            continue  # rejected at parse time; rule 2 allows either stage
        if manifest.validate().is_valid():
            rule.failures.append((name, "accepted with no errors"))
    return rule


def rule_valid_murls(suite: Path) -> Rule:
    rule = Rule(3, "valid mURLs parse and round-trip", minimum=15)
    for line in _read_lines(suite, "valid.txt"):
        if not line:
            continue
        rule.note(line)
        try:
            parsed = parse_murl(line)
        except MurlSyntaxError as exc:
            rule.failures.append((line, f"failed to parse: {exc}"))
            continue
        canonical = parsed.canonical
        try:
            reparsed = parse_murl(canonical)
        except MurlSyntaxError as exc:
            rule.failures.append(
                (line, f"canonical form {canonical!r} failed to reparse: {exc}")
            )
            continue
        if reparsed != parsed:
            rule.failures.append(
                (line, f"did not round-trip: canonical {canonical!r} reparsed differently")
            )
    return rule


def rule_invalid_murls(suite: Path) -> Rule:
    rule = Rule(4, "invalid mURLs are rejected", minimum=20)
    for line in _read_lines(suite, "invalid.txt"):
        rule.note(line or "<empty line>")
        try:
            parse_murl(line)
        except MurlSyntaxError:
            continue
        rule.failures.append((line or "<empty line>", "was accepted"))
    return rule


def rule_canonical(suite: Path) -> Rule:
    """Rule 5: each input canonicalizes to exactly its .expected bytes.

    Added after this implementation passed rules 1-4 with a canonical form
    that nothing had ever checked. Signatures are the one place where
    "close enough" means "verifies nowhere else".
    """
    rule = Rule(5, "canonical form matches byte-for-byte", minimum=12)
    directory = suite / "canonical"
    if not directory.is_dir():
        rule.failures.append(("canonical/", "directory is missing"))
        return rule
    for path in sorted(directory.glob("*.input.json")):
        stem = path.name[: -len(".input.json")]
        rule.note(stem)
        expected_path = directory / (stem + ".expected")
        if not expected_path.exists():
            rule.failures.append((stem, "has no .expected file"))
            continue
        expected = expected_path.read_bytes()
        try:
            value = json.loads(path.read_bytes().decode("utf-8"))
            actual = canonical_bytes(value)
        except Exception as exc:  # noqa: BLE001 - report, never crash the run
            rule.failures.append((stem, f"canonicalization failed: {exc}"))
            continue
        if actual != expected:
            rule.failures.append(
                (stem, f"expected {expected!r}, got {actual!r}")
            )
    return rule


RULES: List[Callable[[Path], Rule]] = [
    rule_valid_manifests,
    rule_invalid_manifests,
    rule_valid_murls,
    rule_invalid_murls,
    rule_canonical,
]


def main(argv: List[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--suite",
        type=Path,
        default=DEFAULT_SUITE,
        help="path to spec/conformance (default: the one in this repository)",
    )
    parser.add_argument(
        "-v", "--verbose", action="store_true", help="list every vector checked"
    )
    args = parser.parse_args(argv)

    suite = args.suite.resolve()
    print(f"mURL conformance suite: {suite}")
    print()

    results = [run(suite) for run in RULES]
    failed = 0
    for rule in results:
        status = "PASS" if rule.ok else "FAIL"
        print(f"[{status}] rule {rule.number}: {rule.title}")
        print(f"         {rule.passed}/{rule.checked} vectors")
        if rule.checked < rule.minimum:
            # A wrong path must fail loudly rather than pass vacuously over an
            # empty directory -- the same guard the Rust harness uses.
            noun = "vector" if rule.checked == 1 else "vectors"
            print(
                f"         only {rule.checked} {noun} found, expected at least "
                f"{rule.minimum} -- is the suite path right?"
            )
        if args.verbose:
            broken = {name for name, _ in rule.failures}
            for name in rule.names:
                print(f"           {'FAIL' if name in broken else 'ok  '}  {name}")
        for name, why in rule.failures:
            print(f"         - {name}: {why}")
        if not rule.ok:
            failed += 1

    total = sum(r.checked for r in results)
    total_failed = sum(len(r.failures) for r in results)
    print()
    if failed == 0:
        print(f"PASS: {total} vectors, {len(RULES)}/{len(RULES)} rules")
        return 0
    print(f"FAIL: {total_failed} of {total} vectors failed across {failed} rule(s)")
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
