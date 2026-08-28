"""murl_ref -- a second implementation of the mURL *format*, from the spec.

Scope is deliberately narrow. This package implements three things:

* :mod:`murl_ref.parser`    -- the ``murl://`` grammar (spec 3)
* :mod:`murl_ref.manifest`  -- manifest parsing and validation (spec 5)
* :mod:`murl_ref.canonical` -- MCF-1, the canonical byte form (spec 7.1)

It implements no resolution, no dispatch, and no cryptography. See the README
in this directory for why, and for the list of questions the specification
left open while this was being written.

Pure standard library, no dependencies.
"""

from .canonical import (
    CanonicalError,
    canonical_bytes,
    canonicalize,
    signing_bytes,
)
from .manifest import (
    MAX_MANIFEST_BYTES,
    SUPPORTED_MURL_VERSIONS,
    Manifest,
    ManifestError,
    Report,
    parse_manifest,
)
from .parser import MAX_MURL_BYTES, Murl, MurlSyntaxError, parse_murl

__all__ = [
    "CanonicalError",
    "canonical_bytes",
    "canonicalize",
    "signing_bytes",
    "MAX_MANIFEST_BYTES",
    "SUPPORTED_MURL_VERSIONS",
    "Manifest",
    "ManifestError",
    "Report",
    "parse_manifest",
    "MAX_MURL_BYTES",
    "Murl",
    "MurlSyntaxError",
    "parse_murl",
]

__version__ = "0.2.0"
FORMAT_VERSION = "0.2"
