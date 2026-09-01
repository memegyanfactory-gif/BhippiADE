<p align="center">
  <img src="crates/bhippi-app/icons/icon.png" width="96" alt="Bhippi logo" />
</p>

<h1 align="center">Bhippi ADE</h1>

<p align="center">
  A local-first, AI-native desktop development environment with a Rust-owned game engine and an Unreal-inspired editor.
</p>

<p align="center">
  <a href="https://github.com/memegyanfactory-gif/bhippiADE/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/memegyanfactory-gif/bhippiADE/actions/workflows/ci.yml/badge.svg" /></a>
  <img alt="Rust 1.85+" src="https://img.shields.io/badge/Rust-1.85%2B-CE412B?logo=rust" />
  <img alt="Tauri 2" src="https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white" />
  <img alt="License AGPL-3.0-only" src="https://img.shields.io/badge/license-AGPL--3.0--only-blue" />
</p>

> [!IMPORTANT]
> Bhippi is under active development. The Windows desktop path is the primary development target; CI checks Rust on Windows, macOS, and Linux. Some engine hardening and the wider autonomous research-to-publishing roadmap remain in progress.

## What Bhippi is

Bhippi brings AI agents, project workspaces, source editing, game-engine authoring, deterministic validation, and playable runtime inspection into one desktop application. Humans and agents work through the same typed engine boundary: reads are explicit, writes are transactional, changes are journaled, and authored project data remains recoverable.

The repository contains two connected product tracks:

- **Bhippi ADE and engine workbench:** multi-session AI development, project tools, an Unreal-inspired editor, structured game content, play mode, and bounded autonomous verification.
- **Research and publishing engine:** a local-first architecture for evidence-backed technology research, knowledge graphs, editorial gates, and static publishing. This track is earlier in its build order.

## Highlights

### AI-native desktop workspace

- Project-scoped Chat and CLI sessions with independent drafts, providers, models, and live state.
- Local and CLI provider discovery, streaming responses, usage accounting, typed faults, and explicit capability controls.
- Built-in workbench, deterministic debugger, command palette, activity history, and optional computer-use controls.
- No silent cloud fallback: offline and provider state are shown truthfully.

### Rust-owned game engine

- One transaction path for human and AI scene changes, with validation, undo/redo, journaling, and crash recovery.
- Versioned scene, HUD, material, shader, prefab, input, and game-manifest formats.
- Asset indexing, provenance and licence sidecars, deterministic procedural helpers, and release-blocking content gates.
- Typed AI queries and action batches, scene leases, capability policies, screenshots, playtests, repair attempts, and golden autonomy fixtures.

### Unreal-inspired editing and play

- World Outliner, schema-driven Details panel, Content Browser, viewport toolbar, quick open, command palette, Output Log, and transport controls.
- Three.js viewport with meshes, PBR materials, lights, weather, collider diagnostics, transform hierarchy, and selected-camera preview.
- Disposable play worlds with physics, character control, input mapping, compiled Rhai-subset scripts, runtime HUD, level travel, pause, step, restart, and time scaling.
- Play mode never mutates the authored scene; Stop returns to the original document.

## Architecture

```text
React UI / Three.js viewport
          │ generated Tauri IPC
          ▼
bhippi-app ── sessions, permissions, observations, desktop integration
          │
          ├── bhippi-engine ── documents, transactions, schemas, play composition
          ├── bhippi-core   ── orchestration, budgets, events, cancellation
          ├── bhippi-db     ── SQLite repositories and journals
          └── bhippi-providers ── local, CLI and API model adapters
```

Business rules live in Rust. TypeScript renders state, runs the isolated viewport/play presentation layer, and sends typed requests through generated IPC bindings.

## Quick start

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) stable, version 1.85 or newer
- [Node.js](https://nodejs.org/) 22 and npm
- Platform dependencies required by [Tauri 2](https://v2.tauri.app/start/prerequisites/)
- On Windows, the Microsoft Edge WebView2 runtime

### Build and run

```bash
git clone https://github.com/memegyanfactory-gif/bhippiADE.git
cd bhippiADE
npm ci --prefix ui
npm run build --prefix ui
cargo run -p bhippi-app --bin bhippi-desktop
```

Bhippi stores runtime configuration and local state outside the repository. Provider secrets belong in the operating-system keychain; never commit them to this project.

## Quality gates

Run the same checks expected by CI:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm test --prefix ui
npm run build --prefix ui
```

Regenerate typed IPC bindings after changing the Tauri command surface:

```bash
cargo run -p bhippi-app --bin export-bindings
git diff --exit-code -- ui/src/lib/ipc.ts
```

## Repository map

```text
crates/bhippi-app/          Tauri desktop shell and application seams
crates/bhippi-engine/       Headless engine domain and transaction authority
crates/bhippi-engine-build/ Build preflight, packaging and release gates
crates/bhippi-core/         Orchestration, events, budgets and cancellation
crates/bhippi-db/           SQLite migrations and repositories
crates/bhippi-providers/    Model discovery and provider adapters
ui/                         React editor shell and Three.js viewport
tests/fixtures/engine/      Deterministic engine and release fixtures
prompts/                    Versioned model-facing instructions
docs/                       Product specification, contracts, plans and ADRs
```

## Documentation

- [Documentation index](docs/README.md)
- [Architecture](docs/01-ARCHITECTURE.md)
- [Module contracts](docs/02-MODULE-CONTRACTS.md)
- [Invariant register](docs/06-INVARIANTS.md)
- [Build order](docs/08-BUILD-ORDER.md)
- [Engine AI control and UX plan](docs/13-ENGINE-AI-CONTROL-AND-UNREAL-UX-PLAN.md)
- [Current progress](docs/PROGRESS.md)
- [Architecture decisions](docs/adr/)

## Security and design principles

- Secrets are stored only in the OS keychain and scrubbed from logs and replay data.
- Engine edits cross a Rust transaction boundary and are journaled with their actor and label.
- Generated or imported release assets require resolved licensing metadata.
- Gameplay scripts are compiled in Rust and executed under deterministic step and call-depth limits.
- Telemetry is off by default and no telemetry network path exists in v1.
- Safety, licence, accessibility, and release checks block; they do not silently warn and continue.

See [SECURITY.md](SECURITY.md) for vulnerability reporting.

## Contributing

Bhippi uses specification-led development. Before changing a module, read its contract and named invariants, then include tests and documentation in the same change. Start with [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Bhippi is licensed under the GNU Affero General Public License v3.0 only (`AGPL-3.0-only`). See [LICENSE](LICENSE).
