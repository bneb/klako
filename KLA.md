# KLA.md: Operational Axioms

This manifest outlines the immutable truths governing Klako agent interaction within this repository. To stray from these principles is to invite nondeterminism.

## Detected Modalities
- **Primary Language:** Safe Rust.
- **Frameworks:** None. We eschew superfluous abstractions in our foundational layers.

## Verification Constraints
Any proposed behavioral mutation must survive our verification gauntlet:
- Execute from the `rust/` boundary: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
- Any alterations to operational behavior must see synchronized validation across both `src/` and `tests/` when modifying legacy Python layers. The topology demands it.

## Structural Topology
- `rust/`: The absolute source of truth holding the precise CLI and runtime mechanisms.
- `src/`: Legacy Python scaffolding that must remain consistent with downstream tests.
- `tests/`: Empirical verification planes corresponding to `src/`.

## Working Agreements
- Refactoring must be atomic, logically coherent, and meticulously scoped.
- Shared configuration is centrally defined in `.kla.json`. Local transient perturbations belong securely in `.kla/settings.local.json`.
- The contents of `KLA.md` are sacred. Do not programmatically overwrite this file. Update it only through intentional acts of human consensus when our repo workflows genuinely require evolution.
