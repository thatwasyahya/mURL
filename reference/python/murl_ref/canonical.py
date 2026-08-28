"""MCF-1, the mURL canonical form — spec 7.1.

This is the byte form that hashes and signatures cover. Everything about it is
chosen to be boring:

* object members sorted by Unicode code point of the member name;
* no insignificant whitespace;
* the seven named escapes, remaining control characters as ``\\u00xx``, and
  everything else raw UTF-8;
* integers only, in plain decimal.

The float rejection is the load-bearing part. MCF-1 is byte-identical to
RFC 8785 (JCS) for every document the manifest schema allows, and it gets
there by removing the one piece of JCS that is genuinely hard to reimplement:
ECMAScript number formatting. A canonical form that is easy to get subtly
wrong produces signatures that break for reasons nobody can reproduce.

Two details the prose in 7.1 does not pin down, resolved here to match the
reference implementation (see the README's "Spec questions this raised"):

* the hex in ``\\u00xx`` is **lowercase**, as RFC 8785 requires;
* only code points below 0x20 are escaped -- U+007F is emitted raw, again as
  RFC 8785 does.
"""

from __future__ import annotations

from typing import Any, Dict

__all__ = [
    "CanonicalError",
    "canonicalize",
    "canonical_bytes",
    "signing_bytes",
    "INT_MIN",
    "INT_MAX",
]


class CanonicalError(ValueError):
    """The value cannot be represented in MCF-1."""


INT_MIN = -(2**63)
INT_MAX = 2**64 - 1

_SHORT_ESCAPES = {
    '"': '\\"',
    "\\": "\\\\",
    "\b": "\\b",
    "\f": "\\f",
    "\n": "\\n",
    "\r": "\\r",
    "\t": "\\t",
}


def _write_string(text: str, out: list) -> None:
    out.append('"')
    for char in text:
        escape = _SHORT_ESCAPES.get(char)
        if escape is not None:
            out.append(escape)
        elif ord(char) < 0x20:
            out.append("\\u%04x" % ord(char))
        else:
            out.append(char)
    out.append('"')


def _write(value: Any, out: list) -> None:
    if value is None:
        out.append("null")
    elif value is True:
        out.append("true")
    elif value is False:
        out.append("false")
    elif isinstance(value, int):
        # bool was handled above; Python's bool-is-int would otherwise emit 1/0.
        if not INT_MIN <= value <= INT_MAX:
            raise CanonicalError(f"integer {value} does not fit i64/u64")
        out.append(str(value))
    elif isinstance(value, float):
        raise CanonicalError(f"canonical form forbids non-integer numbers ({value})")
    elif isinstance(value, str):
        _write_string(value, out)
    elif isinstance(value, (list, tuple)):
        out.append("[")
        for index, item in enumerate(value):
            if index:
                out.append(",")
            _write(item, out)
        out.append("]")
    elif isinstance(value, dict):
        out.append("{")
        # Python compares str by code point, which is the ordering 7.1 asks
        # for. (RFC 8785 sorts by UTF-16 code unit; the two agree for every
        # member name below U+10000, which is every name the schema permits.)
        for index, key in enumerate(sorted(value)):
            if not isinstance(key, str):
                raise CanonicalError(f"object member name {key!r} is not a string")
            if index:
                out.append(",")
            _write_string(key, out)
            out.append(":")
            _write(value[key], out)
        out.append("}")
    else:
        raise CanonicalError(f"value of type {type(value).__name__} is not JSON")


def canonicalize(value: Any) -> str:
    """Return the MCF-1 text for a JSON value."""
    out: list = []
    _write(value, out)
    return "".join(out)


def canonical_bytes(value: Any) -> bytes:
    """Return the MCF-1 bytes for a JSON value. This is what gets hashed."""
    return canonicalize(value).encode("utf-8")


def signing_bytes(document: Dict[str, Any]) -> bytes:
    """The bytes a signature covers (7.2): the manifest minus ``signature``.

    Note what is *not* removed: unknown members. Signatures cover them (5.1),
    so a consumer that ignores a member still verifies the bytes that carried
    it. This function only prepares those bytes -- it does no cryptography.
    """
    if not isinstance(document, dict):
        raise CanonicalError("a manifest document must be an object")
    stripped = {k: v for k, v in document.items() if k != "signature"}
    return canonical_bytes(stripped)
