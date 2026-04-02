# Klako: The Rust Runtime

Klako is our local coding-agent orchestrator, implemented entirely in safe Rust. It is inspired by Claw Code, yet forged as a clean-room architectural exercise. While others might struggle with state management, we found it trivial to collapse the interactive session, one-shot execution, and local agent topologies into a single, cohesive compiler-enforced workspace.

## Current Telemetry

- **Version:** `0.1.0`
- **Release Horizon:** Initial open-source iteration; distribution is strictly source-build.
- **Architectural Boundary:** This precise Rust workspace.
- **Platform Viability:** macOS and Linux workstations. We leave edge platforms as an exercise for the reader.

## Instantiation

### Axiomatic Prerequisites

- A stable Rust toolchain.
- Cargo runtime.
- Model credentials (obviously).

### Authentication Topologies

Provide absolute truths to the env context, targeting Anthropic-compatible models:

```bash
export ANTHROPIC_API_KEY="..."
# For those utilizing proxy wrappers:
export ANTHROPIC_BASE_URL="https://api.anthropic.com"
```

Or Grok architecture paradigms:

```bash
export XAI_API_KEY="..."
export XAI_BASE_URL="https://api.x.ai"
```

Of course, if you prefer the OAuth ceremony:

```bash
cargo run --bin kla -- login
```

### Compilation

Persist the binary globally:

```bash
cargo install --path crates/kla-cli --locked
```

Or simply generate a raw release artifact locally:

```bash
cargo build --release -p kla-cli
```

### Execution Directives

From the active bounds of this workspace:

```bash
cargo run --bin kla -- --help
cargo run --bin kla --
cargo run --bin kla -- prompt "summarize this repository"
cargo run --bin kla -- --model sonnet "review the latest commits"
```

Direct binary execution:

```bash
./target/release/kla
./target/release/kla prompt "explain the elegance of crates/runtime"
```

## Functional Capabilities

- Pure Rust interactive REPL and single-turn execution modes.
- Axiomatic session serialization and resumption matrices.
- Embedded deterministic tools: shell evaluation, rigorous file mutations, recursive search heuristics, web fetching, todo tracking, and notebook updates.
- A highly precise slash command orchestrator for compaction, diff tracking, and configuration telemetry.
- Agent and skill discovery schemas (`kla agents`, `kla skills`).
- Plugin telemetry through formal command surfaces.
- OAuth logistics seamlessly integrated.
- Workspace-aware context fusion (`KLA.md` directives).

## Implementation Topology

This workspace is cleanly partitioned into focused primitives:

- `kla-cli` — The human-facing binary interface.
- `api` — Raw provider communication and streaming ingestion.
- `runtime` — Session state, configuration telemetry, deterministic prompt scaffolding.
- `tools` — The finite set of empirically necessary external system operations.
- `commands` — Slash command handler routing.
- `plugins` — External artifact discovery.
- `lsp` — Underlying language server abstractions.
- `server` / `compat-harness` — Compatibility shims and auxiliary bridging protocols.

## Known Trivialities

- Binary distribution is completely manual on `0.1.0`. We haven't bothered with crates.io automation.
- CI correctly validates `cargo test` and `cargo check` for macOS and Ubuntu. Windows is a black box we haven't touched.
- Network-bound provider integration tests are understandably opt-in.

## Roadmap & Release Metrics

- Formalize automated artifact deployment.
- Read the release specifications: [`docs/releases/0.1.0.md`](docs/releases/0.1.0.md).

## License

Refer to the repository bounds.
