# Contributing to Klako

We appreciate your inclination to contribute to the Klako monolith. While the architecture is theoretically flawless, we welcome empirical improvements. Please adhere to the following axioms; they are non-negotiable.

## Operational Constraints

- You must employ the stable Rust toolchain. Alpha features are amusing, but we require deterministic stability.
- All operations must be confined to the `rust/` workspace boundary. If you spawned in the primary repository root, navigate downward accordingly (`cd rust/`).

## Compilation

```bash
cargo build
cargo build --release
```

## The Verification Gauntlet

Before you burden us with a pull request, you are expected to mathematically prove your changes via our native verification suite:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo check --workspace
cargo test --workspace
```

If your proposition alters fundamental behavioral paradigms, you are required to synthesize corresponding verification tests in the same diff.

## Aesthetic Symmetry

- Assimilate into the pre-existing tectonic plates of the crate you are modifying. We have no interest in your idiosyncratic formatting preferences.
- Silence all the complaints rendered by `rustfmt`.
- Eradicate every single `clippy` warning. We compile with `-D warnings` for a reason.
- Keep your diffs hyper-focused. Drive-by refactors are an irritating distraction.

## The Pull Request Paradigm

- Fork appropriately and branch from `main`.
- Isolate your pull request to a single, unambiguous cognitive shift.
- Your opening proposition must clearly elucidate the underlying motivation, the mechanical execution, and the exact verification routines you executed.
- If your localized verification fails, do not ask for review.
- Any behavioral divergence requested during peer review requires a complete rerun of the verification gauntlet.
