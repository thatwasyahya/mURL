"""Manifest parsing and validation — spec §5.

Two stages, kept deliberately separate:

``parse_manifest``  enforces the envelope (§5.1): size cap before parsing,
                    UTF-8 JSON, top-level object, and — the interesting one —
                    rejection of duplicate object members at every nesting
                    level. That last rule is why this module hand-feeds
                    ``json.load`` an ``object_pairs_hook`` instead of using the
                    convenient ``json.loads``: Python's default silently keeps
                    the last duplicate, which is exactly the
                    signature-confusion behaviour §5.1 exists to forbid.

``validate``        checks the schema (§5.2-5.4) and returns a report rather
                    than raising, because a validator that stops at the first
                    error is a poor authoring tool.

Validation is *static*. It never reads the clock, the filesystem, or the
network: ``notBefore``/``expires`` are checked for format and ordering, not
against now. Time-of-use is a resolution concern (§8.3), which this
implementation does not implement at all.
"""

from __future__ import annotations

import base64
import binascii
import datetime
import json
import re
from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional

from .parser import (
    RE_RESOURCE_ID,
    RE_ROLE,
    RE_TAG,
    MurlSyntaxError,
    parse_murl,
)

__all__ = [
    "ManifestError",
    "Report",
    "Manifest",
    "parse_manifest",
    "MAX_MANIFEST_BYTES",
    "SUPPORTED_MURL_VERSIONS",
]


class ManifestError(ValueError):
    """The manifest could not be parsed at all (§5.1 envelope violation)."""


# §5.1 / §6.6
MAX_MANIFEST_BYTES = 262_144

# §9. Pre-1.0 the accepted set is enumerated rather than range-checked.
SUPPORTED_MURL_VERSIONS = ("0.1", "0.2")

MAX_RESOURCES = 64
MAX_RELATIONS = 128
MAX_DEPENDS_ON = 16
MAX_TAGS = 16
MAX_NAME_CHARS = 120
MAX_DESCRIPTION_CHARS = 2000
MAX_TARGET_BYTES = 2048
MAX_ORDER = 10_000

BUILTIN_KINDS = frozenset(
    {
        "https",
        "file",
        "dir",
        "murl",
        "terminal",
        "ssh",
        "remote-desktop",
        "geo",
        "mailto",
    }
)

TOP_LEVEL_MEMBERS = frozenset(
    {
        "murlVersion",
        "id",
        "name",
        "description",
        "version",
        "notBefore",
        "expires",
        "resources",
        "relations",
        "signature",
    }
)
RESOURCE_MEMBERS = frozenset(
    {
        "id",
        "kind",
        "target",
        "label",
        "role",
        "required",
        "order",
        "dependsOn",
        "tags",
        "integrity",
        "meta",
    }
)
RELATION_MEMBERS = frozenset({"from", "rel", "to"})
SIGNATURE_MEMBERS = frozenset({"alg", "keyId", "publicKey", "sig"})

RE_CUSTOM_KIND = re.compile(r"\A[a-z0-9][a-z0-9_-]{0,31}\Z")
RE_REL = re.compile(r"\A[a-z][a-z-]{0,31}\Z")
RE_TIMESTAMP = re.compile(r"\A(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})Z\Z")
RE_CONTENT_VERSION = re.compile(r"\A(?:0|[1-9][0-9]{0,4})(?:\.(?:0|[1-9][0-9]{0,4}))*\Z")
RE_KEY_ID = re.compile(r"\Aed25519:[0-9a-f]{16}\Z")

# §7.1: numbers must fit i64 or u64. The union of the two ranges is what a
# canonicalizer on either side of the wire can represent without loss.
INT_MIN = -(2**63)
INT_MAX = 2**64 - 1


class NonIntegerNumber:
    """Placeholder for a JSON number that was not an integer.

    §5.1 requires validators to *report* non-integer numbers as errors, so they
    are captured at parse time and surfaced during validation rather than
    aborting the parse. Keeping them in the tree also means one bad float does
    not hide the rest of a document's problems.
    """

    __slots__ = ("text",)

    def __init__(self, text: str) -> None:
        self.text = text

    def __repr__(self) -> str:  # pragma: no cover - diagnostics only
        return f"NonIntegerNumber({self.text!r})"


def _reject_duplicates(pairs):
    """``object_pairs_hook`` implementing §5.1's duplicate-member rule.

    Two conformant consumers can verify the same signature and then act on
    different values if one keeps the first duplicate and the other the last
    (threat T-15). The only safe reading of such a document is "refuse it",
    and refusing has to happen here — before any member is interpreted.
    """
    seen = set()
    for key, _ in pairs:
        if key in seen:
            raise ManifestError(f"duplicate object member {key!r}")
        seen.add(key)
    return dict(pairs)


def _reject_constant(name: str):
    raise ManifestError(f"{name} is not valid JSON in a manifest")


def parse_manifest(data: bytes, *, max_bytes: int = MAX_MANIFEST_BYTES) -> Dict[str, Any]:
    """Envelope stage (§5.1). Returns the raw document or raises."""
    if not isinstance(data, (bytes, bytearray)):
        raise ManifestError("manifest input must be bytes")
    # §5.1: the size cap is enforced *before* parsing. A parser is a poor place
    # to discover you were handed a gigabyte.
    if len(data) > max_bytes:
        raise ManifestError(f"manifest is {len(data)} bytes, limit is {max_bytes}")
    try:
        text = bytes(data).decode("utf-8")
    except UnicodeDecodeError as exc:
        raise ManifestError(f"manifest is not valid UTF-8: {exc}") from None
    try:
        doc = json.loads(
            text,
            object_pairs_hook=_reject_duplicates,
            parse_float=NonIntegerNumber,
            parse_constant=_reject_constant,
        )
    except ManifestError:
        raise
    except json.JSONDecodeError as exc:
        raise ManifestError(f"malformed JSON: {exc}") from None
    if not isinstance(doc, dict):
        raise ManifestError("the top-level value must be an object")
    return doc


@dataclass
class Report:
    """The outcome of validation. Errors reject; warnings never do."""

    errors: List[str] = field(default_factory=list)
    warnings: List[str] = field(default_factory=list)

    def is_valid(self) -> bool:
        return not self.errors

    def error(self, message: str) -> None:
        self.errors.append(message)

    def warn(self, message: str) -> None:
        self.warnings.append(message)


class Manifest:
    """A parsed manifest document, plus the §5 validation rules."""

    def __init__(self, doc: Dict[str, Any]) -> None:
        self.doc = doc

    @classmethod
    def from_bytes(
        cls, data: bytes, *, max_bytes: int = MAX_MANIFEST_BYTES
    ) -> "Manifest":
        return cls(parse_manifest(data, max_bytes=max_bytes))

    # -- validation ---------------------------------------------------------

    def validate(self) -> Report:
        report = Report()
        doc = self.doc

        _check_numbers(doc, "<root>", report)
        _check_unknown(doc, TOP_LEVEL_MEMBERS, "manifest", report)

        _validate_murl_version(doc, report)
        _validate_name(doc, report)
        _validate_description(doc, report)
        _validate_id(doc, report)
        _validate_content_version(doc, report)
        _validate_validity_window(doc, report)
        ids = _validate_resources(doc, report)
        _validate_relations(doc, ids, report)
        _validate_signature(doc, report)

        return report


# --- top-level members ------------------------------------------------------


def _is_str(value: Any) -> bool:
    return isinstance(value, str)


def _has_control(text: str) -> bool:
    return any(ord(c) < 0x20 or ord(c) == 0x7F for c in text)


def _check_unknown(obj: Dict[str, Any], known, where: str, report: Report) -> None:
    """§5.1/§11: unknown members are ignored, and surfaced as warnings.

    They are *not* errors — that is the whole forward-compatibility story — but
    silence would make a typo indistinguishable from a future feature.
    """
    for key in obj:
        if key not in known:
            report.warn(f"{where}: unknown member {key!r} ignored")


def _check_numbers(value: Any, path: str, report: Report) -> None:
    """§5.1/§7.1: every number anywhere in the document must be an integer.

    "Anywhere" includes ``meta``, which is otherwise opaque: MCF-1 has to
    canonicalize the whole document, including the parts the resolver never
    reads.
    """
    if isinstance(value, NonIntegerNumber):
        report.error(f"{path}: {value.text} is not an integer; manifests forbid floats")
    elif isinstance(value, bool):
        pass  # bool is a subclass of int in Python; JSON true/false is not a number
    elif isinstance(value, int):
        if not INT_MIN <= value <= INT_MAX:
            report.error(f"{path}: integer {value} does not fit i64/u64")
    elif isinstance(value, dict):
        for key, item in value.items():
            _check_numbers(item, f"{path}.{key}", report)
    elif isinstance(value, list):
        for index, item in enumerate(value):
            _check_numbers(item, f"{path}[{index}]", report)


def _validate_murl_version(doc: Dict[str, Any], report: Report) -> None:
    value = doc.get("murlVersion")
    if value is None:
        report.error("manifest: 'murlVersion' is required")
    elif not _is_str(value):
        report.error("manifest: 'murlVersion' must be a string")
    elif value not in SUPPORTED_MURL_VERSIONS:
        report.error(
            f"manifest: unsupported murlVersion {value!r}; "
            f"this implementation accepts {', '.join(SUPPORTED_MURL_VERSIONS)}"
        )


def _validate_name(doc: Dict[str, Any], report: Report) -> None:
    value = doc.get("name")
    if value is None:
        report.error("manifest: 'name' is required")
    elif not _is_str(value):
        report.error("manifest: 'name' must be a string")
    elif not 1 <= len(value) <= MAX_NAME_CHARS:
        report.error(f"manifest: 'name' must be 1..{MAX_NAME_CHARS} characters")
    elif _has_control(value):
        report.error("manifest: 'name' contains a control character")


def _validate_description(doc: Dict[str, Any], report: Report) -> None:
    if "description" not in doc:
        return
    value = doc["description"]
    if not _is_str(value):
        report.error("manifest: 'description' must be a string")
    elif len(value) > MAX_DESCRIPTION_CHARS:
        report.error(f"manifest: 'description' exceeds {MAX_DESCRIPTION_CHARS} characters")
    elif _has_control(value):
        report.error("manifest: 'description' contains a control character")


def _validate_id(doc: Dict[str, Any], report: Report) -> None:
    """§5.2/§6.4: ``id`` binds a manifest to the name it may be served under."""
    if "id" not in doc:
        if "signature" in doc:
            # §6.4: a signed manifest with no 'id' can be re-labelled -- served
            # validly signed under a name its author never intended. Unsigned
            # manifests get no warning; there is nothing to replay.
            report.warn(
                "manifest: signed but carries no 'id'; nothing prevents this "
                "manifest being served under a different name (§6.4)"
            )
        return
    value = doc["id"]
    if not _is_str(value):
        report.error("manifest: 'id' must be a string")
        return
    try:
        parsed = parse_murl(value)
    except MurlSyntaxError as exc:
        report.error(f"manifest: 'id' is not a valid mURL: {exc}")
        return
    # §3.3: identity is the canonical form with query and selector stripped, and
    # identity is what §6.4 compares. An 'id' carrying either is binding the
    # manifest to something that is not a name.
    if parsed.selector is not None:
        report.error("manifest: 'id' must not carry a selector; identity strips it")
    if parsed.query is not None:
        report.error("manifest: 'id' must not carry a query; identity strips it")
    if parsed.selector is None and parsed.query is None and value != parsed.canonical:
        report.warn(f"manifest: 'id' is not in canonical form ({parsed.canonical!r})")


def _validate_content_version(doc: Dict[str, Any], report: Report) -> None:
    if "version" not in doc:
        return
    value = doc["version"]
    if not _is_str(value):
        report.error("manifest: 'version' must be a string")
    elif value == "latest":
        # §5.2: 'latest' is a *name* version alias (§9), never content. A
        # manifest that calls its own content "latest" says nothing at all.
        report.error("manifest: 'version' must never be 'latest'")
    elif not RE_CONTENT_VERSION.match(value):
        report.error(f"manifest: 'version' {value!r} is not dotted integers")


def _validate_timestamp(value: Any, member: str, report: Report) -> Optional[str]:
    """§5.2: strict ``YYYY-MM-DDTHH:MM:SSZ``. No offsets, no fractional seconds.

    One spelling means one parser, and one parser means two implementations
    cannot disagree about what instant a signed manifest expires at.
    """
    if not _is_str(value):
        report.error(f"manifest: {member!r} must be a string")
        return None
    match = RE_TIMESTAMP.match(value)
    if not match:
        report.error(
            f"manifest: {member!r} must be a strict UTC timestamp "
            f"YYYY-MM-DDTHH:MM:SSZ, got {value!r}"
        )
        return None
    year, month, day, hour, minute, second = (int(g) for g in match.groups())
    try:
        datetime.datetime(year, month, day, hour, minute, second)
    except ValueError as exc:
        report.error(f"manifest: {member!r} is not a real instant: {exc}")
        return None
    return value


def _validate_validity_window(doc: Dict[str, Any], report: Report) -> None:
    not_before = expires = None
    if "notBefore" in doc:
        not_before = _validate_timestamp(doc["notBefore"], "notBefore", report)
    if "expires" in doc:
        expires = _validate_timestamp(doc["expires"], "expires", report)
    if not_before is not None and expires is not None and not_before >= expires:
        # Lexicographic comparison is exact for this fixed-width UTC format.
        report.error("manifest: 'notBefore' must be strictly before 'expires'")


# --- resources --------------------------------------------------------------


def _validate_resources(doc: Dict[str, Any], report: Report) -> List[str]:
    value = doc.get("resources")
    if value is None:
        report.error("manifest: 'resources' is required")
        return []
    if not isinstance(value, list):
        report.error("manifest: 'resources' must be an array")
        return []
    if not 1 <= len(value) <= MAX_RESOURCES:
        report.error(
            f"manifest: 'resources' must hold 1..{MAX_RESOURCES} entries, "
            f"got {len(value)}"
        )
        if not value:
            return []

    ids: List[str] = []
    seen = set()
    for index, resource in enumerate(value):
        where = f"resources[{index}]"
        if not isinstance(resource, dict):
            report.error(f"{where}: must be an object")
            continue
        _check_unknown(resource, RESOURCE_MEMBERS, where, report)
        rid = _validate_resource_id(resource, where, seen, report)
        if rid is not None:
            ids.append(rid)
        kind = _validate_kind(resource, where, report)
        _validate_target(resource, kind, where, report)
        _validate_optional_resource_members(resource, kind, where, report)

    _validate_depends_on(value, ids, report)
    return ids


def _validate_resource_id(resource, where, seen, report) -> Optional[str]:
    rid = resource.get("id")
    if rid is None:
        report.error(f"{where}: 'id' is required")
        return None
    if not _is_str(rid):
        report.error(f"{where}: 'id' must be a string")
        return None
    if not RE_RESOURCE_ID.match(rid):
        report.error(
            f"{where}: 'id' {rid!r} must match [a-z0-9][a-z0-9_-]{{0,63}}"
        )
        return None
    if rid in seen:
        # Ids are the selector's addressing space (§6.7); a duplicate makes
        # '#that-id' mean two things.
        report.error(f"{where}: duplicate resource id {rid!r}")
        return None
    seen.add(rid)
    return rid


def _validate_kind(resource, where, report) -> Optional[str]:
    kind = resource.get("kind")
    if kind is None:
        report.error(f"{where}: 'kind' is required")
        return None
    if not _is_str(kind):
        report.error(f"{where}: 'kind' must be a string")
        return None
    if kind in BUILTIN_KINDS:
        return kind
    if kind.startswith("custom:"):
        if not RE_CUSTOM_KIND.match(kind[len("custom:") :]):
            report.error(f"{where}: malformed custom kind {kind!r}")
            return None
        return kind
    # §11: new built-in kinds require a spec revision, so an unknown bare kind
    # is an error rather than a forward-compatibility warning. Extensions go
    # through 'custom:', where dispatch requires a locally registered handler.
    report.error(f"{where}: unknown kind {kind!r}")
    return None


def _validate_optional_resource_members(resource, kind, where, report) -> None:
    if "label" in resource:
        label = resource["label"]
        if not _is_str(label):
            report.error(f"{where}: 'label' must be a string")
        elif not 1 <= len(label) <= MAX_NAME_CHARS:
            report.error(f"{where}: 'label' must be 1..{MAX_NAME_CHARS} characters")
        elif _has_control(label):
            report.error(f"{where}: 'label' contains a control character")

    if "role" in resource:
        role = resource["role"]
        if not _is_str(role) or not RE_ROLE.match(role):
            report.error(
                f"{where}: 'role' {role!r} must match [a-z0-9][a-z0-9-]{{0,31}}"
            )

    if "required" in resource and not isinstance(resource["required"], bool):
        report.error(f"{where}: 'required' must be a boolean")

    if "order" in resource:
        order = resource["order"]
        if isinstance(order, bool) or not isinstance(order, int):
            report.error(f"{where}: 'order' must be an integer")
        elif not 0 <= order <= MAX_ORDER:
            report.error(f"{where}: 'order' {order} outside 0..{MAX_ORDER}")

    if "tags" in resource:
        tags = resource["tags"]
        if not isinstance(tags, list):
            report.error(f"{where}: 'tags' must be an array")
        elif len(tags) > MAX_TAGS:
            report.error(f"{where}: more than {MAX_TAGS} tags")
        else:
            for tag in tags:
                if not _is_str(tag) or not RE_TAG.match(tag):
                    report.error(f"{where}: tag {tag!r} must match [a-z0-9-]{{1,32}}")

    if "integrity" in resource:
        _validate_integrity(resource["integrity"], kind, where, report)


def _validate_integrity(value, kind, where, report) -> None:
    """§5.3: ``sha256-<base64>`` over the raw bytes of a nested manifest."""
    if not _is_str(value):
        report.error(f"{where}: 'integrity' must be a string")
        return
    prefix = "sha256-"
    if not value.startswith(prefix):
        report.error(f"{where}: 'integrity' must begin with {prefix!r}")
        return
    digest = _decode_base64(value[len(prefix) :], 32)
    if digest is None:
        report.error(f"{where}: 'integrity' is not base64 of a 32-byte SHA-256 digest")
        return
    if kind is not None and kind != "murl":
        # §5.3 says the pin is "only enforced for kind: murl". Carrying one
        # elsewhere is inert, not malformed — worth saying out loud so an
        # author does not believe a file is pinned when it is not.
        report.warn(f"{where}: 'integrity' is only enforced for kind 'murl'; ignored")


def _validate_depends_on(resources: List[Any], ids: List[str], report: Report) -> None:
    """§5.3: ≤16 known ids per resource, and the whole graph must be acyclic."""
    known = set(ids)
    edges: Dict[str, List[str]] = {}
    for index, resource in enumerate(resources):
        if not isinstance(resource, dict):
            continue
        where = f"resources[{index}]"
        rid = resource.get("id")
        deps = resource.get("dependsOn")
        if deps is None:
            continue
        if not isinstance(deps, list):
            report.error(f"{where}: 'dependsOn' must be an array")
            continue
        if len(deps) > MAX_DEPENDS_ON:
            report.error(f"{where}: more than {MAX_DEPENDS_ON} dependsOn entries")
        clean: List[str] = []
        for dep in deps:
            if not _is_str(dep):
                report.error(f"{where}: 'dependsOn' entries must be strings")
                continue
            if dep not in known:
                report.error(f"{where}: 'dependsOn' names unknown resource {dep!r}")
                continue
            if dep == rid:
                report.error(f"{where}: resource {dep!r} depends on itself")
                continue
            clean.append(dep)
        if _is_str(rid):
            edges[rid] = clean

    cycle = _find_cycle(edges)
    if cycle is not None:
        report.error(
            "manifest: 'dependsOn' graph has a cycle: " + " -> ".join(cycle)
        )


def _find_cycle(edges: Dict[str, List[str]]) -> Optional[List[str]]:
    """Iterative DFS. Returns one cycle if the graph has any."""
    WHITE, GREY, BLACK = 0, 1, 2
    color: Dict[str, int] = {node: WHITE for node in edges}
    for start in list(edges):
        if color[start] != WHITE:
            continue
        stack = [(start, iter(edges.get(start, ())))]
        path = [start]
        color[start] = GREY
        while stack:
            node, children = stack[-1]
            advanced = False
            for child in children:
                state = color.get(child, BLACK)
                if state == GREY:
                    return path[path.index(child) :] + [child]
                if state == WHITE:
                    color[child] = GREY
                    path.append(child)
                    stack.append((child, iter(edges.get(child, ()))))
                    advanced = True
                    break
            if not advanced:
                color[node] = BLACK
                stack.pop()
                path.pop()
    return None


# --- targets (§5.3) ---------------------------------------------------------


def _validate_target(resource, kind, where, report) -> None:
    target = resource.get("target")
    if target is None:
        report.error(f"{where}: 'target' is required")
        return
    if not _is_str(target):
        report.error(f"{where}: 'target' must be a string")
        return
    size = len(target.encode("utf-8"))
    if not 1 <= size <= MAX_TARGET_BYTES:
        report.error(f"{where}: 'target' must be 1..{MAX_TARGET_BYTES} bytes")
        return
    if _has_control(target):
        report.error(f"{where}: 'target' contains a control character")
        return
    if kind is None:
        return  # the kind was already reported; a target check would be noise

    checker = {
        "https": _target_https,
        "file": _target_path,
        "dir": _target_path,
        "terminal": _target_path,
        "murl": _target_murl,
        "ssh": _target_ssh,
        "remote-desktop": _target_remote_desktop,
        "geo": _target_geo,
        "mailto": _target_mailto,
    }.get(kind)
    if checker is None:
        # custom:<name> — charset and length rules only. The safety of a custom
        # target comes from the handler the user registered, not from here.
        return
    checker(target, where, report)


def _split_authority(after_scheme: str) -> str:
    for index, char in enumerate(after_scheme):
        if char in "/?#":
            return after_scheme[:index]
    return after_scheme


def _is_loopback(host: str) -> bool:
    return host == "localhost" or host == "127.0.0.1" or host.endswith(".localhost")


def _check_host_port(hostport: str, where: str, report: Report, what: str) -> None:
    host = hostport
    if ":" in hostport:
        host, port_raw = hostport.rsplit(":", 1)
        if not port_raw.isdigit() or not 1 <= int(port_raw) <= 65535:
            report.error(f"{where}: {what} port {port_raw!r} is not 1..65535")
    if not host:
        report.error(f"{where}: {what} has an empty host")
    elif host.startswith("-"):
        # A leading '-' would read as a command-line option to whatever handler
        # receives the target.
        report.error(f"{where}: {what} host {host!r} must not begin with '-'")
    elif not re.fullmatch(r"[A-Za-z0-9.-]+", host):
        report.error(f"{where}: {what} host {host!r} has invalid characters")


def _target_https(target: str, where: str, report: Report) -> None:
    if " " in target:
        report.error(f"{where}: https target contains a space")
        return
    if target.startswith("https://"):
        authority = _split_authority(target[len("https://") :])
    elif target.startswith("http://"):
        authority = _split_authority(target[len("http://") :])
        host = authority.rsplit(":", 1)[0] if ":" in authority else authority
        if not _is_loopback(host):
            report.error(
                f"{where}: plain http is only permitted for loopback hosts, "
                f"got {host!r}"
            )
            return
    else:
        report.error(f"{where}: https target must use https:// (or loopback http://)")
        return
    if "@" in authority:
        # The same phishing shape §3.2 forbids in the mURL itself.
        report.error(f"{where}: userinfo is forbidden in an https target")
        return
    _check_host_port(authority, where, report, "https target")


def _target_path(target: str, where: str, report: Report) -> None:
    if target == "~" or target.startswith("~/"):
        pass
    elif target.startswith("/"):
        pass
    elif re.match(r"\A[A-Za-z]:[\\/]", target):
        pass
    else:
        report.error(
            f"{where}: path target must be absolute ('/…', 'X:\\…', 'X:/…') "
            f"or '~'-rooted; got {target!r}"
        )
        return
    for part in re.split(r"[\\/]", target):
        if part in (".", ".."):
            report.error(f"{where}: dot segment in path target {target!r}")
            return


def _target_murl(target: str, where: str, report: Report) -> None:
    try:
        parsed = parse_murl(target)
    except MurlSyntaxError as exc:
        report.error(f"{where}: murl target is not a valid mURL: {exc}")
        return
    if parsed.selector is not None:
        # §5.3: a selector on a nested mURL has no defined semantics. Ignoring
        # it silently would let an author believe they had narrowed a nested
        # destination when the whole thing gets spliced in.
        report.warn(f"{where}: selector on a nested mURL has no meaning; ignored")


def _target_ssh(target: str, where: str, report: Report) -> None:
    prefix = "ssh://"
    if not target.startswith(prefix):
        report.error(f"{where}: ssh target must use ssh://")
        return
    rest = target[len(prefix) :]
    if "/" in rest:
        report.error(f"{where}: ssh target must be ssh://[user@]host[:port] with no path")
        return
    if rest.count("@") > 1:
        report.error(f"{where}: ssh target has more than one '@'")
        return
    if "@" in rest:
        user, rest = rest.split("@", 1)
        if user.startswith("-"):
            report.error(f"{where}: ssh username {user!r} must not begin with '-'")
            return
        if not re.fullmatch(r"[A-Za-z0-9._-]+", user):
            report.error(f"{where}: ssh username {user!r} must match [A-Za-z0-9._-]+")
            return
    _check_host_port(rest, where, report, "ssh target")


def _target_remote_desktop(target: str, where: str, report: Report) -> None:
    for prefix in ("rdp://", "vnc://"):
        if target.startswith(prefix):
            rest = target[len(prefix) :]
            break
    else:
        report.error(f"{where}: remote-desktop target must use rdp:// or vnc://")
        return
    if "@" in rest:
        report.error(f"{where}: userinfo is forbidden in a remote-desktop target")
        return
    if "/" in rest:
        report.error(f"{where}: remote-desktop target must be scheme://host[:port]")
        return
    _check_host_port(rest, where, report, "remote-desktop target")


def _target_geo(target: str, where: str, report: Report) -> None:
    """§5.3: ``geo:lat,lon[,alt][;param]`` per RFC 5870, with range checks."""
    if not target.startswith("geo:"):
        report.error(f"{where}: geo target must begin with 'geo:'")
        return
    body = target[len("geo:") :].split(";", 1)[0]
    parts = body.split(",")
    if not 2 <= len(parts) <= 3:
        report.error(
            f"{where}: geo target needs 'lat,lon' with optional altitude, got {body!r}"
        )
        return
    numbers = []
    for part in parts:
        if not re.fullmatch(r"-?\d+(?:\.\d+)?", part):
            report.error(f"{where}: geo coordinate {part!r} is not a decimal number")
            return
        numbers.append(float(part))
    if not -90.0 <= numbers[0] <= 90.0:
        report.error(f"{where}: geo latitude {parts[0]} outside -90..90")
    if not -180.0 <= numbers[1] <= 180.0:
        report.error(f"{where}: geo longitude {parts[1]} outside -180..180")


MAILTO_HEADERS = frozenset({"subject", "body", "cc", "to"})


def _target_mailto(target: str, where: str, report: Report) -> None:
    """§5.3: RFC 6068, restricted to headers that cannot add hidden recipients."""
    if not target.startswith("mailto:"):
        report.error(f"{where}: mailto target must begin with 'mailto:'")
        return
    rest = target[len("mailto:") :]
    addrs, _, headers = rest.partition("?")
    if not addrs:
        report.error(f"{where}: mailto target has no address")
        return
    for addr in addrs.split(","):
        if addr.count("@") != 1:
            report.error(f"{where}: {addr!r} is not an addr-spec")
            return
        local, domain = addr.split("@", 1)
        if not local or not domain:
            report.error(f"{where}: {addr!r} is not an addr-spec")
            return
        if not re.fullmatch(r"[A-Za-z0-9.-]+", domain) or domain.startswith("."):
            report.error(f"{where}: {addr!r} has a malformed domain")
            return
    if headers:
        for pair in headers.split("&"):
            key, sep, _ = pair.partition("=")
            if not sep:
                report.error(f"{where}: malformed mailto header {pair!r}")
                return
            if key.lower() not in MAILTO_HEADERS:
                # bcc is the point of this rule: a manifest may pre-fill a
                # message, but must not be able to add recipients the user
                # will not see before sending.
                report.error(
                    f"{where}: mailto header {key!r} is not one of "
                    f"{', '.join(sorted(MAILTO_HEADERS))}"
                )
                return


# --- relations and signature ------------------------------------------------


def _validate_relations(doc: Dict[str, Any], ids: List[str], report: Report) -> None:
    if "relations" not in doc:
        return
    value = doc["relations"]
    if not isinstance(value, list):
        report.error("manifest: 'relations' must be an array")
        return
    if len(value) > MAX_RELATIONS:
        report.error(f"manifest: more than {MAX_RELATIONS} relations")
    known = set(ids)
    for index, relation in enumerate(value):
        where = f"relations[{index}]"
        if not isinstance(relation, dict):
            report.error(f"{where}: must be an object")
            continue
        _check_unknown(relation, RELATION_MEMBERS, where, report)
        for member in ("from", "to"):
            endpoint = relation.get(member)
            if not _is_str(endpoint):
                report.error(f"{where}: {member!r} is required and must be a string")
            elif endpoint not in known:
                report.error(f"{where}: {member!r} names unknown resource {endpoint!r}")
        rel = relation.get("rel")
        if not _is_str(rel) or not RE_REL.match(rel):
            report.error(f"{where}: 'rel' {rel!r} must match [a-z][a-z-]{{0,31}}")


def _decode_base64(text: str, expected_len: int) -> Optional[bytes]:
    if not isinstance(text, str):
        return None
    try:
        raw = base64.b64decode(text, validate=True)
    except (binascii.Error, ValueError):
        return None
    if len(raw) != expected_len:
        return None
    return raw


def _validate_signature(doc: Dict[str, Any], report: Report) -> None:
    """§7.2: the *shape* of the signature block.

    This is a static check only. Verifying the signature means recomputing
    MCF-1 bytes and running ed25519 over them, which belongs to resolution and
    is out of scope for this implementation (see the README).
    """
    if "signature" not in doc:
        return
    block = doc["signature"]
    if not isinstance(block, dict):
        report.error("manifest: 'signature' must be an object")
        return
    _check_unknown(block, SIGNATURE_MEMBERS, "signature", report)

    alg = block.get("alg")
    if not _is_str(alg):
        report.error("signature: 'alg' is required and must be a string")
    elif alg != "ed25519":
        report.error(f"signature: unsupported algorithm {alg!r}; only 'ed25519'")

    key_id = block.get("keyId")
    if not _is_str(key_id):
        report.error("signature: 'keyId' is required and must be a string")
    elif not RE_KEY_ID.match(key_id):
        report.error(
            f"signature: 'keyId' {key_id!r} must be 'ed25519:' plus 16 lowercase "
            "hex digits"
        )

    if "publicKey" not in block:
        report.error("signature: 'publicKey' is required")
    elif _decode_base64(block["publicKey"], 32) is None:
        report.error("signature: 'publicKey' must be base64 of exactly 32 bytes")

    if "sig" not in block:
        report.error("signature: 'sig' is required")
    elif _decode_base64(block["sig"], 64) is None:
        report.error("signature: 'sig' must be base64 of exactly 64 bytes")
