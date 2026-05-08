# Klako: Tier-1 Autonomous CLI Engine

Klako is a high-performance, local-first agent orchestrator. It is designed to be superior to existing AI CLIs by leveraging deterministic Rust kernels, strategic sub-agent orchestration, and persistent Bayesian memory.

## 🚀 System Architecture (V2)

Klako is built on a "World-Kernel" architecture, where complex reasoning is offloaded from the LLM to high-fidelity Rust simulation engines.

### **The "Worlds" Ecosystem**
- **SportsWorld**: SOTA minute-by-minute discrete event simulation for NBA, MLB, UCL, and ATP. Includes tactical modeling, stamina decay, and set-piece gravity.
- **LogisticsWorld**: Itinerary optimization using TSP heuristics and psychological cognitive load constraints.
- **ActuarialWorld**: Parallelized Monte Carlo system for risk assessment and Bayesian parameter sampling.
- **NutritionWorld**: Clinical-grade meal planning with daily diary gap-closure and micronutrient tracking.
- **DiscoveryWorld & SymbolWorld**: Context-efficient structural mapping and exact semantic code navigation (LSP-lite).
- **MemoryWorld**: Persistent, key-based long-term brain for global and project-specific priors.

## 🧠 Strategic Orchestration
Klako uses a hierarchical delegation model. A primary orchestrator (powered by Gemma 4 26B MoE or Gemini 3.1) delegates specialized tasks to expert sub-agents:
- **Explore**: Architectural discovery and semantic symbol lookup.
- **Logistics**: High-precision scheduling and optimization.
- **Verification**: Red-to-green TDD auditing and idiomatic parity checks.

## 📊 Visual Intelligence
Klako produces isomorphic visual artifacts. High-density data is rendered natively as **ANSI-rich ASCII charts** in the Terminal and as **Interactive Canvas components** in the Browser Notebook UI.

## 🛠 Engineering Excellence (SRE/Staff)
- **Deterministic**: Every simulation is seed-controlled and perfectly reproducible.
- **Parallelized**: Utilizing `rayon` for multi-core performance.
- **Verified**: Strictly TDD-driven with property-based fuzzing via `proptest`.
- **Safe**: Mandatory "Plan Mode" and structural "Parity Audits" prevent destructive file edits.

## 🏗 Build & Launch

Rebuild the system to synchronize the dynamic schemas and modular kernels:

```bash
cd rust && cargo build --release -p kla-cli
```

Launch the Notebook UI:
```bash
./target/release/kla notebook
```

---
*Built for absolute perfection and mathematical undeniability.*
