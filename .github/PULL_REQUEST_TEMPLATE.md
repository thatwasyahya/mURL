## What

<!-- One paragraph: what this PR changes. -->

## Why

<!-- The reasoning. Link the issue if there is one. -->

## Spec impact

- [ ] No format/resolution behavior change
- [ ] Changes format or resolution semantics — `spec/SPECIFICATION.md` and
      relevant `docs/` are updated in this PR

## Security impact

<!-- Does this touch the parser, validator, resolver, policy, trust,
     dispatch, fetcher, or consent paths? If yes: which threat-model entries
     (docs/threat-model.md) are relevant, and which tests demonstrate the
     property is preserved? -->

## Checklist

- [ ] `cargo fmt` clean
- [ ] `cargo clippy --all-targets` warning-free
- [ ] `cargo test` passes (new behavior has new tests)
- [ ] `cd fuzz && cargo check` passes; fuzz run if hostile-input paths changed
- [ ] No new dependencies, or their case is made in "Why"
