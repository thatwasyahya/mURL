"""The ``murl://`` grammar — spec §3.

A parser is the first thing an attacker touches, so this one is written to be
read: every rule below cites the clause it enforces, and there is no repair
path anywhere. A malformed mURL raises; it is never coerced into a
plausible-looking one, because a parser that guesses is a parser that can be
steered (§3.2).

This module implements syntax only. It knows nothing about name stores,
manifests, or the network.
"""

from __future__ import annotations

import re
from dataclasses import dataclass
from typing import Optional, Tuple

__all__ = [
    "MurlSyntaxError",
    "Murl",
    "parse_murl",
    "MAX_MURL_BYTES",
]


class MurlSyntaxError(ValueError):
    """An input that violates spec §3. Carries the rule that rejected it."""


# --- §3.2 constraints -------------------------------------------------------

MAX_MURL_BYTES = 1024
MAX_NAME_SEGMENTS = 8
MAX_SEGMENT_BYTES = 64
MAX_QUERY_CHARS = 512
MAX_SELECTOR_ITEMS = 8

SCHEME = "murl://"

# §3.1 shared field grammars. These are the *same* definitions the manifest
# validator uses (spec §5.2-5.3 explicitly shares them), which is why they
# live here and are imported there rather than being written twice.
RE_RESOURCE_ID = re.compile(r"\A[a-z0-9][a-z0-9_-]{0,63}\Z")
RE_ROLE = re.compile(r"\A[a-z0-9][a-z0-9-]{0,31}\Z")
RE_TAG = re.compile(r"\A[a-z0-9-]{1,32}\Z")

RE_LABEL = re.compile(r"\A[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\Z")
RE_VNUM = re.compile(r"\A(?:0|[1-9][0-9]{0,4})\Z")  # 1*5DIGIT, no leading zeros

# Unreserved set used by the canonical form (§3.3).
_UNRESERVED = frozenset(
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~"
)
_HEXDIG = frozenset("0123456789abcdefABCDEF")


def _pct_encode(segment: str) -> str:
    """Minimal percent-encoding of one decoded segment (§3.3).

    Unreserved characters stay literal; every other byte becomes ``%XX`` with
    uppercase hex. Encoding operates on UTF-8 bytes, not characters.
    """
    out = []
    for byte in segment.encode("utf-8"):
        char = chr(byte)
        if char in _UNRESERVED:
            out.append(char)
        else:
            out.append("%%%02X" % byte)
    return "".join(out)


def _pct_decode(raw: str, *, where: str) -> bytes:
    """Decode one raw segment to bytes, rejecting malformed escapes.

    ``%`` is not a ``seg-char``, so every ``%`` in the input must introduce a
    complete two-digit escape.
    """
    out = bytearray()
    i = 0
    n = len(raw)
    while i < n:
        char = raw[i]
        if char == "%":
            if i + 3 > n:
                raise MurlSyntaxError(f"{where}: truncated percent-escape {raw[i:]!r}")
            hi, lo = raw[i + 1], raw[i + 2]
            if hi not in _HEXDIG or lo not in _HEXDIG:
                raise MurlSyntaxError(
                    f"{where}: malformed percent-escape {raw[i:i + 3]!r}"
                )
            out.append(int(hi + lo, 16))
            i += 3
        else:
            # seg-char = %x21-7E except "/" "?" "#" "@" "%". The delimiters have
            # already been split off by the caller, so only "%" can appear here
            # outside an escape, and it is handled above.
            code = ord(char)
            if not 0x21 <= code <= 0x7E:
                raise MurlSyntaxError(f"{where}: character {char!r} is not a seg-char")
            out.append(code)
            i += 1
    return bytes(out)


def _has_control(text: str) -> bool:
    return any(ord(c) < 0x20 or ord(c) == 0x7F for c in text)


@dataclass(frozen=True)
class Murl:
    """A parsed, normalized mURL.

    Fields hold the *decoded, normalized* value, so two mURLs that name the
    same destination compare equal regardless of how they were spelled. That
    is what makes the round-trip property in the conformance suite meaningful:
    ``parse(str(parse(x))) == parse(x)``.
    """

    authority: str  # lowercased; the reserved word "local" or a DNS host
    port: Optional[int]
    segments: Tuple[str, ...]  # decoded name segments
    version: Optional[str]  # None means unpinned; "@latest" normalizes to None
    query: Optional[str]
    selector: Optional[Tuple[str, ...]]  # raw selector items, in order

    # -- canonical form and identity (§3.3) ---------------------------------

    @property
    def canonical(self) -> str:
        """Lowercased scheme/authority, minimally re-encoded name, no ``@latest``."""
        host = self.authority if self.port is None else f"{self.authority}:{self.port}"
        out = SCHEME + host + "/" + "/".join(_pct_encode(s) for s in self.segments)
        if self.version is not None:
            out += "@" + self.version
        if self.query is not None:
            out += "?" + self.query
        if self.selector is not None:
            out += "#" + ",".join(self.selector)
        return out

    @property
    def identity(self) -> str:
        """Canonical form with query and selector stripped (§3.3).

        The unit of cache keying, cycle detection, and the §6.4 ``id`` binding
        check. Two mURLs with the same identity name the same manifest.
        """
        host = self.authority if self.port is None else f"{self.authority}:{self.port}"
        out = SCHEME + host + "/" + "/".join(_pct_encode(s) for s in self.segments)
        if self.version is not None:
            out += "@" + self.version
        return out

    @property
    def name(self) -> str:
        """The decoded name, segments joined by ``/``."""
        return "/".join(self.segments)

    def __str__(self) -> str:  # pragma: no cover - trivial
        return self.canonical


# --- parsing ----------------------------------------------------------------


def parse_murl(text: str) -> Murl:
    """Parse one mURL string, or raise :class:`MurlSyntaxError`.

    The order of checks is deliberate: cheap whole-input bounds first, then
    structure, then per-component grammar. Nothing is normalized before it has
    been validated.
    """
    if not isinstance(text, str):
        raise MurlSyntaxError("mURL must be a string")

    # §3.2: total length and character set. Doing this first means every later
    # rule can assume printable ASCII, which is why the grammar below never has
    # to think about homoglyphs or bidi controls.
    encoded_len = len(text.encode("utf-8"))
    if encoded_len > MAX_MURL_BYTES:
        raise MurlSyntaxError(f"mURL is {encoded_len} bytes, limit is {MAX_MURL_BYTES}")
    for char in text:
        if not 0x21 <= ord(char) <= 0x7E:
            raise MurlSyntaxError(
                f"non-printable-ASCII character {char!r}; "
                "non-ASCII must be percent-encoded UTF-8"
            )

    if text[: len(SCHEME)].lower() != SCHEME:
        raise MurlSyntaxError("must begin with 'murl://'")
    rest = text[len(SCHEME) :]

    # Split trailing components off first: selector, then query. Neither may
    # contain the delimiter that introduces it, so first-occurrence wins.
    selector_raw = None
    if "#" in rest:
        rest, selector_raw = rest.split("#", 1)
    query = None
    if "?" in rest:
        rest, query = rest.split("?", 1)

    slash = rest.find("/")
    if slash < 0:
        raise MurlSyntaxError("missing '/' between authority and name")
    authority_raw, path = rest[:slash], rest[slash + 1 :]

    authority, port = _parse_authority(authority_raw)
    segments, version = _parse_path(path)
    selector = _parse_selector(selector_raw) if selector_raw is not None else None
    if query is not None:
        _check_query(query)

    return Murl(
        authority=authority,
        port=port,
        segments=segments,
        version=version,
        query=query,
        selector=selector,
    )


def _parse_authority(raw: str) -> Tuple[str, Optional[int]]:
    """authority = "local" / host [ ":" port ]  (§3.1)."""
    if not raw:
        raise MurlSyntaxError("empty authority")
    if "@" in raw:
        # §3.2: userinfo is forbidden precisely so that
        # murl://github.com@evil.example/x is a parse error and not a phishing
        # vector that renders as "github.com" to a hurried reader.
        raise MurlSyntaxError("userinfo is forbidden in the authority")
    if "[" in raw or "]" in raw:
        raise MurlSyntaxError("IPv6 literals are not supported")

    lowered = raw.lower()  # §3.2: authority case folds to lowercase

    host, port = lowered, None
    if ":" in lowered:
        if lowered.count(":") > 1:
            raise MurlSyntaxError("multiple ':' in authority")
        host, port_raw = lowered.rsplit(":", 1)
        if not (1 <= len(port_raw) <= 5) or not port_raw.isdigit():
            raise MurlSyntaxError(f"malformed port {port_raw!r}")
        port = int(port_raw)
        if not 1 <= port <= 65535:
            raise MurlSyntaxError(f"port {port} outside 1..65535")

    if host == "local":
        # "local" is the reserved word branch of the grammar, not a host, so it
        # takes no port: murl://local:80/x names nothing, because the local
        # store has no port to listen on.
        if port is not None:
            raise MurlSyntaxError("the reserved authority 'local' takes no port")
        return host, None

    if not host:
        raise MurlSyntaxError("empty host")
    for label in host.split("."):
        if not label:
            raise MurlSyntaxError(f"empty label in host {host!r}")
        if len(label) > 63:
            raise MurlSyntaxError(f"host label {label!r} exceeds 63 characters")
        if not RE_LABEL.match(label):
            raise MurlSyntaxError(
                f"host label {label!r} is not 1*63(lc-alnum / '-') "
                "without leading or trailing '-'"
            )
    return host, port


def _parse_path(path: str) -> Tuple[Tuple[str, ...], Optional[str]]:
    """name [ "@" version ]  (§3.1), returning decoded segments and version."""
    at_count = path.count("@")
    if at_count > 1:
        raise MurlSyntaxError("'@' may appear at most once, as the version marker")
    version = None
    if at_count == 1:
        name_part, version_raw = path.split("@", 1)
        if "/" in version_raw:
            # §3.2: the version marker attaches to the final segment only.
            raise MurlSyntaxError("'@' may only appear on the final name segment")
        version = _parse_version(version_raw)
        path = name_part

    raw_segments = path.split("/")
    if not 1 <= len(raw_segments) <= MAX_NAME_SEGMENTS:
        raise MurlSyntaxError(
            f"name has {len(raw_segments)} segments, limit is {MAX_NAME_SEGMENTS}"
        )

    segments = []
    for raw in raw_segments:
        if not raw:
            raise MurlSyntaxError("empty name segment")
        decoded_bytes = _pct_decode(raw, where="name segment")
        if len(decoded_bytes) > MAX_SEGMENT_BYTES:
            raise MurlSyntaxError(
                f"name segment decodes to {len(decoded_bytes)} bytes, "
                f"limit is {MAX_SEGMENT_BYTES}"
            )
        try:
            decoded = decoded_bytes.decode("utf-8")
        except UnicodeDecodeError as exc:
            raise MurlSyntaxError(f"name segment is not valid UTF-8: {exc}") from None
        if _has_control(decoded):
            raise MurlSyntaxError("control character in decoded name segment")
        if "/" in decoded or "\\" in decoded:
            # An encoded separator is an attempt to smuggle structure past the
            # segment split — into a store path or a well-known URL.
            raise MurlSyntaxError("decoded name segment contains a path separator")
        if decoded in (".", ".."):
            raise MurlSyntaxError("dot segments are forbidden, encoded or not")
        segments.append(decoded)

    return tuple(segments), version


def _parse_version(raw: str) -> Optional[str]:
    """version = "latest" / vnum *2( "." vnum )  (§3.1).

    ``latest`` normalizes to ``None``: §3.3 elides it from the canonical form,
    so the pinned-to-nothing and explicitly-latest spellings must compare equal
    or the round-trip property fails.
    """
    if not raw:
        raise MurlSyntaxError("empty version after '@'")
    if raw == "latest":
        return None
    parts = raw.split(".")
    if len(parts) > 3:
        raise MurlSyntaxError("version has more than three components")
    for part in parts:
        if not RE_VNUM.match(part):
            raise MurlSyntaxError(
                f"version component {part!r} is not 1*5DIGIT without leading zeros"
            )
    return raw


def _parse_selector(raw: str) -> Tuple[str, ...]:
    """selector = sel-item *7( "," sel-item )  (§3.1), union semantics (§6.7)."""
    if not raw:
        raise MurlSyntaxError("empty selector after '#'")
    items = raw.split(",")
    if len(items) > MAX_SELECTOR_ITEMS:
        raise MurlSyntaxError(
            f"selector has {len(items)} items, limit is {MAX_SELECTOR_ITEMS}"
        )
    for item in items:
        if not item:
            raise MurlSyntaxError("empty selector item")
        if item.startswith("role="):
            if not RE_ROLE.match(item[len("role=") :]):
                raise MurlSyntaxError(f"malformed role selector {item!r}")
        elif item.startswith("tag="):
            if not RE_TAG.match(item[len("tag=") :]):
                raise MurlSyntaxError(f"malformed tag selector {item!r}")
        elif not RE_RESOURCE_ID.match(item):
            raise MurlSyntaxError(f"malformed resource-id selector {item!r}")
    return tuple(items)


def _check_query(query: str) -> None:
    """query = *512qchar (§3.1). Reserved: preserved, never interpreted."""
    if len(query) > MAX_QUERY_CHARS:
        raise MurlSyntaxError(
            f"query is {len(query)} characters, limit is {MAX_QUERY_CHARS}"
        )
