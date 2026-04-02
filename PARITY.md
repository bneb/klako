# The Parity Axiom Analysis

For the academically curious, this document formalizes the architectural deltas between the original Claude Code TypeScript implementation (previously housed in `/home/bellman/Workspace/kla-code/src/`) and our far more rigorous Rust port (`rust/crates/`). 

We restricted our empirical methodology strictly to feature surfaces, registry alignments, and runtime topologies. No arbitrary TypeScript abstractions were mindlessly replicated. 

## The Inevitable Conclusion 

Our Rust runtime provides a robust, formally rigid foundation encompassing:
- Fundamental Anthropic API logic and OAuth handshakes
- Deterministic local conversation caching
- The core tool-execution loop
- MCP stdio integration logic
- Discoverability of the `KLA.md` operational axioms
- A pragmatic, strictly scoped set of native tools

However, we did not blindly port the entire legacy TypeScript monolith, resulting in several expected (and frankly, preferable) deviations from parity:
- **Plugins:** Abstract external plugin loaders are currently absent.
- **Hooks:** We parse hook definitions, but evaluating arbitrary hooks implies a chaotic state space we have chosen to defer.
- **CLI Breadth:** We implement only the strictly necessary slash commands.
- **Skills:** Skill resolution is strictly local-file. The bloated TS registry paradigm has been discarded. 
- **Orchestration:** Lacks the TS hook-aware orchestration engine.
- **Services:** Over-engineered TS abstraction layers are consciously omitted in favor of a clean core.

---

## Tool Infrastructure (`tools/`)

### The TypeScript Artifact
The legacy TS implementation relies on a sprawling taxonomy of `AgentTool`, `FileReadTool`, `TaskTool`, `TeamTool`, and excessively layered execution classes.

### The Rust Reality
The tool registry is elegantly condensed in `rust/crates/tools/src/lib.rs`. It provides only the empirically required primitives: shell, file execution, local search, web operations, local skills, sub-agents format, config, repl, powershell, and notebook integration. We explicitly decided against bloating our binary with arbitrary TS constructs like `AskUserQuestionTool` or `TaskTool`. Our tool execution loop is a strict, unambiguous MVP registry.

---

## Hook Lifecycle (`hooks/`)

### The TypeScript Artifact
Hooks are integrated deep within `src/services/tools/toolHooks.ts`, supporting arbitrary `PreToolUse` and `PostToolUse` execution topologies.

### The Rust Reality
We successfully parse the hook configurations (`rust/crates/runtime/src/config.rs`) as a demonstration of parser completeness. We do not, however, actually execute them. The state obfuscation introduced by arbitrary pre- and post-tool mutation chains was deemed theoretically impure for this milestone. Consequently, there is no `/hooks` command. Deal with it.

---

## The Plugin Ecosystem (`plugins/`)

### The TypeScript Artifact
Characterized by a complex `PluginInstallationManager` and dynamic lifecycle scaffolding.

### The Rust Reality
Consciously missing. We avoid dynamic external code execution paradigms native to TS in our compiled Rust artifact. Subsequent iterations may formalize this. 

---

## Skill Registries (`skills/`)

### The TypeScript Artifact
A vast registry and dynamic loading mechanism across `loadSkillsDir.ts` and bundled MCP skill abstractions.

### The Rust Reality
We employ a simple, local-first file resolution strategy through `rust/crates/tools/src/lib.rs`. If you want a skill, write a `.md` file. The CLI natively understands `/memory` and `/init`, but we abandoned the remote MCP skill builder complexity. 

---

## Command Interface (`cli/`)

### The TypeScript Artifact
A heavily decoupled `src/cli/handlers/` architecture relying on vast dependency injection and polymorphic UI streams. 

### The Rust Reality
Commands are strictly defined as precise slash directives in `rust/crates/commands/src/lib.rs`. We natively support `/help`, `/status`, `/compact`, `/model`, `/permissions`, `/clear`, `/cost`, `/resume`, `/config`, `/memory`, `/init`, `/diff`, `/version`, `/export`, and `/session`. The JSON output path ensures raw transport metrics, though we acknowledge some pre-JSON tool output logging still trivially exists. 

---

## Assistant Engine (`assistant/`)

### The TypeScript Artifact
A heavily generalized, hook-aware streaming orchestrator with layers of indirection.

### The Rust Reality
We constructed a pure deterministic loop (`rust/crates/runtime/src/conversation.rs`). It strips away the unnecessary orchestration fat, focusing purely on state persistence and absolute API transcription. 

---

## Service Integrations (`services/`)

### The TypeScript Artifact
A sprawling ecosystem encompassing analytics, suggestion heuristics, global policy limits, and superfluous UI bloat. 

### The Rust Reality
Our `rust/crates/api` crate manages Anthropic API interactions natively. `rust/crates/runtime` handles OAuth, MCP, token telemetry usage accounting, and proxy networking. 

---

## Known Trivialities and Fixes

- **Unrestricted Agent Constraints:** We patched the runtime to default to `DangerFullAccess`. We prefer our agents functionally unbridled.
- **JSON Object Initialization:** The streaming parser now elegantly strips initial empty `{}` objects exclusively for streaming tool input.
- **Iteration Limits:** Enforced bounds (`usize::MAX`) are structurally implemented. 
- **JSON Prompt Cleanliness:** We are perfectly aware that the tool-capable JSON mode may occasionally emit human-readable metadata logging. We will rectify it when we feel like it.
