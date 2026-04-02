# Klako: Formalizing Autonomous Agency

While the broader ecosystem occupies itself with empirical trial-and-error, we found it mildly amusing to formalize local coding agency into an axiomatic Rust implementation. Klako is our clean-room, safe Rust instantiation of a Claude Code-style local CLI. It provides an undeniably elegant interactive agent shell, seamless one-shot execution, and flawlessly deterministic workspace-aware tool integrations. The profound simplicity of its architecture speaks for itself.

## Getting Started

Assuming you possess a stable Rust toolchain and the capability to procure your own Anthropic or Grok API credentials, booting the orchestrator is a trivial endeavor.

Move into the orchestrator boundary:
```bash
cd rust
```

Compile the primary binary:
```bash
cargo build --release -p kla-cli
```

Initiate the interactive shell:
```bash
cargo run --bin kla -- --help
cargo run --bin kla --
```

For those inclined toward isolated deterministic outputs:
```bash
cargo run --bin kla -- prompt "explain this monolithic triviality"
```

If you must persist this architecture globally:
```bash
cargo install --path crates/kla-cli --locked
```

## Epistemic Authentication

You are free to authenticate through the CLI interface, or via environment variables if you prefer a more explicit injection of truth:

```bash
cargo run --bin kla -- login
```

For Anthropic models:
```bash
export ANTHROPIC_API_KEY="..."
export ANTHROPIC_BASE_URL="https://api.anthropic.com"
```

For Grok architecture equivalents:
```bash
export XAI_API_KEY="..."
export XAI_BASE_URL="https://api.x.ai"
```

## Core Interaction Topology

The slash command surface exposes all fundamental topological truths of the session state. Run `kla --help` to witness the complete command registry, natively bypassing undocumented assumptions.

```bash
# Evaluate delta and converge on truth
cargo run --bin kla -- prompt "review the latest changes"

# Scaffold invariant configurations
cargo run --bin kla -- init

# Terminate session persistence
cargo run --bin kla -- logout

# Resume a previous cognitive state
cargo run --bin kla -- --resume session.json /status
```

## Abstract Directory Structure

```text
.
├── rust/            # The axiomatic Rust workspace and runtime engine
├── src/             # Auxiliary python verification and legacy ports
├── tests/           # Auxiliary python test suites
├── KLA.md         # Immutable operational theorems for agents
└── README.md        # The document you are currently reading
```

## Verification

If you must contribute, kindly avoid undermining our compile-time guarantees. Navigate to the `rust` directory and assert absolute compliance:

```bash
cd rust
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Axiomatic Truths

- This repository is a clean-room Rust realization of the local-agent paradigm, unequivocally independent from the original Claude Code upstream source.
- `KLA.md` establishes the absolute operational guidelines. Adhere to it, or expect system invariants to break.
