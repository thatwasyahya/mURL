# mURL Conformance Vectors

A shared test suite for the mURL v0.2 format. Its purpose is to make a
*second* implementation cheap to build and verify: run these vectors, and you
know whether you agree with the reference implementation on the cases that
matter.

The suite is versioned with the specification. Vectors encode rules from
`spec/SPECIFICATION.md`; where the spec is ambiguous, the reference
implementation (`crates/murl-core`) is authoritative and the ambiguity is a
spec bug worth reporting.

## Layout

```text
manifests/valid/*.murl.json     must parse AND validate with zero errors
manifests/invalid/*.murl.json   must fail parsing, OR produce >=1 error
murls/valid.txt                 one mURL per line; each must parse
murls/invalid.txt               one mURL per line; each must be rejected
```

Filenames name the feature (valid) or the violated rule (invalid) — a failing
vector tells you what broke without opening it.

## Rules an implementation must satisfy

1. **Valid manifests**: parse successfully and produce no validation errors.
   *Warnings are allowed* — several valid vectors deliberately carry unknown
   members, which must warn (forward compatibility) and never fail.
2. **Invalid manifests**: rejected either at parse time (e.g. duplicate
   object members, which are invalid JSON *for mURL* per spec §5.1) or by
   validation with at least one error. Which of the two is
   implementation-defined; failing to reject at all is a conformance failure.
3. **Valid mURLs**: parse, and **round-trip** — re-parsing the canonical
   form must yield an equal value. Note some vectors are deliberately
   non-canonical on input (uppercase scheme/authority) and must normalize.
4. **Invalid mURLs**: rejected. No repair, no guessing.

Validation is a *static* check: it never consults the clock or the network.
A vector with `notBefore`/`expires` in the past or future is still valid if
its format and ordering are correct — time-of-use policy (spec §8.3) is a
resolution concern, tested separately.

## Running the suite

Against this implementation:

```bash
cargo test -p murl-core --test conformance
```

Against your own: point your test harness at the four locations above and
apply the four rules. The harness in
[`crates/murl-core/tests/conformance.rs`](../../crates/murl-core/tests/conformance.rs)
is ~100 lines and is a fine template.

## Contributing vectors

New vectors are welcome, especially ones that caught a real bug. Keep each
file focused on a single rule, name it after that rule, and add it to the
directory matching the expected outcome. A vector that the reference
implementation disagrees with is a valuable bug report either way — open an
issue rather than adjusting the implementation to match a vector.
