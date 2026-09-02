"""Generate signature conformance vectors from a fixed, published key.

The suite checks the *shape* of a signature block and never checks whether a
signature verifies. That leaves the most interop-critical thing in the format
untested: two implementations agree on MCF-1 only if they also agree on which
bytes get signed, and nothing so far forced that agreement.

The key is deterministic and its seed is in the open on purpose. These
vectors exist to be verified, not to protect anything, and a conformance
suite whose key nobody can reproduce is a suite nobody can extend.
"""
import base64
import hashlib
import io
import json
import os
import subprocess

ROOT = os.path.dirname(os.path.abspath(__file__)) + "/../../../"
OUT = os.path.dirname(os.path.abspath(__file__)) + "/"
os.makedirs(OUT + "valid", exist_ok=True)
os.makedirs(OUT + "invalid", exist_ok=True)

SEED_PHRASE = b"murl conformance vector key v1"
seed = hashlib.sha256(SEED_PHRASE).digest()

keyfile = OUT + "signing-key.json"
io.open(keyfile, "w", encoding="utf-8", newline="\n").write(
    json.dumps(
        {
            "alg": "ed25519",
            # keyId is recomputed by the tool from publicKey; a placeholder
            # here would be ignored, so it is filled in after signing.
            "keyId": "ed25519:0000000000000000",
            "publicKey": "",
            "secretKey": base64.b64encode(seed).decode(),
        },
        indent=2,
    )
    + "\n"
)

murl = ROOT + "target/debug/murl"


def sign(path):
    subprocess.run([murl, "sign", path, "--key", keyfile], check=True, capture_output=True)


BASE = {
    "murlVersion": "0.2",
    "id": "murl://local/conformance/signed",
    "name": "Signed destination",
    "description": "A manifest whose signature must verify in any implementation.",
    "resources": [
        {"id": "docs", "kind": "https", "target": "https://docs.example/x", "role": "docs"},
        {"id": "workspace", "kind": "dir", "target": "~/projects/x", "order": 20},
    ],
}


def write(path, doc):
    io.open(path, "w", encoding="utf-8", newline="\n").write(
        json.dumps(doc, indent=2, ensure_ascii=False) + "\n"
    )


def load(path):
    return json.loads(io.open(path, encoding="utf-8").read())


# ---------------------------------------------------------------- valid ----
p = OUT + "valid/simple.murl.json"
write(p, BASE)
sign(p)
signed = load(p)
public_key = signed["signature"]["publicKey"]
key_id = signed["signature"]["keyId"]
print("public key:", public_key)
print("key id    :", key_id)

# Backfill the key file so it is self-describing.
kf = load(keyfile)
kf["publicKey"] = public_key
kf["keyId"] = key_id
write(keyfile, kf)

# Unicode, astral characters and an unknown member: the signature must cover
# the raw document, so an implementation that re-serializes its own typed
# view before verifying will fail here.
doc = dict(BASE)
doc["name"] = "Signé · 日本語 · 😀"
doc["futureExtension"] = {"unknownToThisVersion": True, "count": 7}
doc["resources"] = BASE["resources"] + [
    {"id": "unicode", "kind": "https", "target": "https://e.example/caf%C3%A9", "label": "café"}
]
p = OUT + "valid/unicode-and-unknown-members.murl.json"
write(p, doc)
sign(p)

# Member order in the file is deliberately not canonical: signing must sort.
doc = {
    "signature": None,
    "resources": BASE["resources"],
    "name": "Reordered on disk",
    "id": "murl://local/conformance/signed",
    "murlVersion": "0.2",
}
del doc["signature"]
p = OUT + "valid/member-order-differs-from-canonical.murl.json"
write(p, doc)
sign(p)

# -------------------------------------------------------------- invalid ----
# Each is a valid signature made invalid one specific way.
base_signed = load(OUT + "valid/simple.murl.json")


def tamper(name, mutate):
    doc = json.loads(json.dumps(base_signed))
    mutate(doc)
    write(OUT + "invalid/" + name, doc)


def set_name(d):
    d["name"] = "Renamed after signing"


def add_resource(d):
    d["resources"].append(
        {"id": "injected", "kind": "https", "target": "https://attacker.example/"}
    )


def change_target(d):
    d["resources"][0]["target"] = "https://attacker.example/"


def flip_sig_byte(d):
    raw = bytearray(base64.b64decode(d["signature"]["sig"]))
    raw[0] ^= 0x01
    d["signature"]["sig"] = base64.b64encode(bytes(raw)).decode()


def other_key(d):
    other = hashlib.sha256(b"murl conformance vector key v1 - different").digest()
    # A syntactically valid but unrelated public key; keyId no longer derives.
    d["signature"]["publicKey"] = base64.b64encode(other).decode()


def keyid_mismatch(d):
    d["signature"]["keyId"] = "ed25519:0123456789abcdef"


def wrong_alg(d):
    d["signature"]["alg"] = "ed448"


def truncated_sig(d):
    raw = base64.b64decode(d["signature"]["sig"])[:32]
    d["signature"]["sig"] = base64.b64encode(raw).decode()


for name, fn in [
    ("name-changed-after-signing.murl.json", set_name),
    ("resource-appended-after-signing.murl.json", add_resource),
    ("target-changed-after-signing.murl.json", change_target),
    ("signature-bit-flipped.murl.json", flip_sig_byte),
    ("public-key-substituted.murl.json", other_key),
    ("keyid-does-not-derive-from-key.murl.json", keyid_mismatch),
    ("unsupported-algorithm.murl.json", wrong_alg),
    ("signature-truncated.murl.json", truncated_sig),
]:
    tamper(name, fn)

print("valid  :", sorted(os.listdir(OUT + "valid")))
print("invalid:", sorted(os.listdir(OUT + "invalid")))
