# KLA.md: Operational Axioms

This manifest outlines the immutable truths governing Klako agent interaction within this repository. To stray from these principles is to invite nondeterminism.

## Detected Modalities
- **Primary Language:** Safe Rust.
- **Frameworks:** None. We eschew superfluous abstractions in our foundational layers.
- **Orchestration:** Autonomous Swarms via the `SwarmOrchestrator` (`/loop`).
- **Logic:** Deterministic Kernels ("Worlds") for non-textual reasoning.

## Operational Axioms

### 1. Deterministic World Primacy
LLMs are creative but logically fallible. For all mathematical, temporal, physical, or structured data simulations, agents MUST delegate to the corresponding `World` tool (e.g., `ActuarialWorld`, `FiscalWorld`). A text-only hallucination of a statistical distribution is a violation of this axiom.

### 2. Swarm Orchestration Integrity
Complex tasks MUST be decomposed by the `SwarmOrchestrator`. Parallel execution via specialized subagents is our standard for problem-solving. No subagent shall operate outside the context of its assigned task manifest.

### 3. Verification Constraints
Any proposed behavioral mutation must survive our verification gauntlet:
- Execute from the `rust/` boundary: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
- Swarm tasks are only "Complete" once empirical proof (tests, tool results) is recorded in the task manifest.
- Any alterations to operational behavior must see synchronized validation across both `src/` and `tests/` when modifying legacy Python layers. The topology demands it.

## Structural Topology
- `rust/`: The absolute source of truth holding the precise CLI and runtime mechanisms.
- `src/`: Legacy Python scaffolding that must remain consistent with downstream tests.
- `tests/`: Empirical verification planes corresponding to `src/`.

## Working Agreements
- Refactoring must be atomic, logically coherent, and meticulously scoped.
- Shared configuration is centrally defined in `.kla.json`. Local transient perturbations belong securely in `.kla/settings.local.json`.
- The contents of `KLA.md` are sacred. Do not programmatically overwrite this file. Update it only through intentional acts of human consensus when our repo workflows genuinely require evolution.
