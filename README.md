<p align="center">
  <img src="ui/public/bhippi-logo.png" width="104" alt="Bhippi logo" />
</p>

<h1 align="center">Bhippi ADE</h1>

<p align="center">
  <strong>Build software and playable worlds with AI, code, browsing, and an Unreal-inspired engine in one local-first desktop workspace.</strong>
</p>

<p align="center">
  <a href="https://github.com/memegyanfactory-gif/BhippiADE/actions/workflows/ci.yml"><img alt="CI status" src="https://github.com/memegyanfactory-gif/BhippiADE/actions/workflows/ci.yml/badge.svg" /></a>
  <img alt="Rust 1.85 or newer" src="https://img.shields.io/badge/Rust-1.85%2B-CE412B?logo=rust" />
  <img alt="Tauri 2" src="https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white" />
  <img alt="React 18" src="https://img.shields.io/badge/React-18-61DAFB?logo=react&logoColor=black" />
  <img alt="License AGPL 3.0 only" src="https://img.shields.io/badge/license-AGPL--3.0--only-blue" />
</p>

<p align="center">
  <a href="#overview">Overview</a> ·
  <a href="#product-tour">Product tour</a> ·
  <a href="#capabilities">Capabilities</a> ·
  <a href="#architecture">Architecture</a> ·
  <a href="#quick-start">Quick start</a> ·
  <a href="#quality-and-safety">Quality</a>
</p>

<p align="center">
  <img src=".github/assets/bhippi-ade-workbench.png" width="100%" alt="Bhippi ADE with an AI conversation beside the game-engine workbench" />
</p>

<p align="center"><em>One desktop application for AI collaboration, project editing, web research, engine authoring, play inspection, and recovery.</em></p>

> [!IMPORTANT]
> Bhippi is under active development. Windows is the primary desktop target today. Core Rust validation also runs on Windows, macOS, and Linux, while advanced production engine backends and platform packaging continue to mature.

## Overview

Bhippi ADE is a local-first, AI-native development environment built with Rust, Tauri, React, and Three.js. It keeps the work that normally jumps between an AI client, editor, browser, terminal, and game engine inside one project-aware application.

The central idea is simple: **humans and AI agents should use the same safe engine boundary**. Reads are explicit, writes are typed and transactional, capability policy is enforced before mutation, and authored project data remains recoverable.

| Surface | What it provides |
| --- | --- |
| **Agent workspace** | Project-scoped conversations, multiple simultaneous sessions, provider and model selection, activity visibility, and reviewable changes. |
| **Editor** | A project tree and source editor for scenes, scripts, materials, manifests, HUD definitions, and supporting files. |
| **Browser** | An integrated research surface that keeps web context beside the active project. |
| **Engine** | An Unreal-inspired workbench with an Outliner, viewport, Details panel, Content Browser, play controls, diagnostics, and build targets. |
| **Rust authority** | Typed actions, transaction validation, journals, recovery, capability controls, deterministic test plans, and release-blocking gates. |

## Product tour

### Start with the project, not an empty chat

Every conversation is scoped to the selected project. Bhippi offers useful starting points for building a feature, planning architecture, exploring the codebase, or auditing bugs, while keeping provider, model, reasoning, and usage controls close to the composer.

<p align="center">
  <img src=".github/assets/agent-workspace.png" width="720" alt="Project-scoped Bhippi agent workspace with task shortcuts and provider controls" />
</p>

### Run multiple AI sessions side by side

Multi mode turns the workspace into an agent operations surface. Different providers or models can inspect the same project in independent sessions, show live work, and keep their file changes reviewable without collapsing everything into one conversation.

<p align="center">
  <img src=".github/assets/multi-agent-workspace.png" width="100%" alt="Four AI sessions running side by side in Bhippi ADE" />
</p>

### Author game worlds visually

The engine workbench combines a searchable World Outliner, a Three.js viewport, camera preview, schema-driven properties, transform tools, recovery controls, and a folder-based Content Browser. The interface is intentionally dense where precision matters and quiet everywhere else.

<p align="center">
  <img src=".github/assets/engine-workbench.png" width="100%" alt="Bhippi game-engine workbench with World Outliner, viewport, Details panel, and Content Browser" />
</p>

### Keep the AI and the engine in the same loop

An agent can reason about the project while the engine remains visible beside it. This makes implementation, inspection, play evidence, and repair follow-up part of one continuous workflow instead of a chain of disconnected tools.

<p align="center">
  <img src=".github/assets/ai-engine-split-view.png" width="100%" alt="Bhippi AI session beside the visible engine viewport and content browser" />
</p>

### Inspect the real project files

The built-in editor exposes the authored files behind the visual tools, including scene JSON, Rhai gameplay scripts, materials, shaders, input maps, weather, HUD data, and the game manifest. Generated recovery data stays visible and auditable.

<p align="center">
  <img src=".github/assets/project-editor.png" width="100%" alt="Bhippi source editor showing a scene document and project file tree" />
</p>

### Research without leaving the workspace

The integrated browser keeps references, documentation, and testing targets close to the editor and engine. Browser state belongs to the workspace rather than interrupting it with a separate application switch.

<p align="center">
  <img src=".github/assets/integrated-browser.png" width="100%" alt="Integrated browser inside the Bhippi project workbench" />
</p>

### Navigate projects with a compact, persistent shell

Agent, Research, Automation, Library, and Plugins remain accessible from the same sidebar. Single and Multi modes let the workspace scale from focused work to coordinated parallel sessions.

<p align="center">
  <img src=".github/assets/project-navigation.png" width="260" alt="Bhippi sidebar with workspace tools and project navigation" />
</p>

## Capabilities

### AI-native workspace

- Project-scoped chat and CLI sessions with independent drafts and live state.
- Single-session focus and multi-session organization for parallel work.
- Provider discovery and selection for installed or configured Claude, Codex, Grok, Kimi, OpenCode, custom, and local-model routes.
- Streaming output, usage visibility, typed provider faults, cancellation, and change review.
- Imported skills and explicit computer-use controls for supported vision-capable providers.
- No silent provider fallback: unavailable and disconnected states are represented truthfully.

### Unreal-inspired engine authoring

- World Outliner, Details panel, Content Browser, viewport toolbar, camera preview, command palette, and Output panel.
- Versioned scene, HUD, material, shader, prefab, input, save, and game-manifest formats.
- Three.js presentation for meshes, PBR materials, lights, weather, hierarchy, collider diagnostics, and selected-camera inspection.
- Disposable play worlds with input mapping, runtime HUD, level travel, pause, step, restart, and time scaling.
- Crash snapshots, autosave recovery, undo/redo, journals, and content provenance.
- Engine-organized content folders for scenes, models, textures, audio, scripts, materials, prefabs, and related assets.

### Safe AI engine control

Bhippi does not give an agent an unrestricted write channel into a game project. Engine work crosses a Rust-owned control layer:

1. The project and active document are observed.
2. Requested work is resolved against a versioned capability registry.
3. Typed actions are validated against policy, budgets, leases, and document state.
4. The complete batch is accepted or rejected before mutation.
5. Accepted changes are journaled with actor and transaction metadata.
6. Verification produces structured evidence that can be reviewed or replayed.

This boundary is shared by visual editor actions and AI actions, which prevents a second, less-safe automation path from quietly developing beside the product.

### Game-aware debugging

`/gamedebug` runs a fixed engine-owned diagnostic pipeline and stores an immutable, AI-ready report under `.bhippi/reports/game-debug/`.

```text
/gamedebug
/gamedebug quick
/gamedebug full
/gamedebug release
/gamedebug full --fix
```

The modes increase diagnostic depth. `--fix` requests bounded repair planning while preserving the normal capability, approval, transaction, and verification boundaries. Reports are surfaced in the Game Debug panel so an agent can follow concrete findings instead of guessing from a screenshot or log fragment.

## Architecture

```text
┌─────────────────────────────────────────────────────────────────────┐
│ React workspace                                                     │
│ Agent sessions · Editor · Browser · Engine UI · Three.js viewport  │
└──────────────────────────────┬──────────────────────────────────────┘
                               │ generated, typed Tauri IPC
                               ▼
┌─────────────────────────────────────────────────────────────────────┐
│ bhippi-app                                                          │
│ Desktop integration · sessions · permissions · observations        │
└───────────────┬──────────────────┬──────────────────┬───────────────┘
                │                  │                  │
                ▼                  ▼                  ▼
┌──────────────────────┐ ┌──────────────────┐ ┌──────────────────────┐
│ bhippi-engine        │ │ bhippi-core      │ │ bhippi-providers     │
│ Documents            │ │ Orchestration    │ │ Local/CLI/API routes │
│ Transactions         │ │ Budgets/events   │ │ Streaming and usage  │
│ Capability registry  │ │ Cancellation     │ │ Typed faults         │
│ Play/debug contracts │ │ Context routing  │ │ Account state        │
└──────────┬───────────┘ └──────────────────┘ └──────────────────────┘
           │
           ▼
┌─────────────────────────────────────────────────────────────────────┐
│ bhippi-db · SQLite repositories, journals, recovery, and metadata  │
└─────────────────────────────────────────────────────────────────────┘
```

Business rules live in Rust. TypeScript renders state, hosts the isolated viewport and play presentation layer, and sends typed requests through generated IPC bindings. SQL remains inside `bhippi-db`; provider secrets remain outside the repository.

### Repository layout

```text
crates/bhippi-app/             Tauri desktop shell and application seams
crates/bhippi-engine/          Headless engine domain and transaction authority
crates/bhippi-engine-build/    Build preflight, export checks, and release gates
crates/bhippi-engine-viewport/ Viewport protocol and presentation contracts
crates/bhippi-core/            Orchestration, context, budgets, and cancellation
crates/bhippi-db/              SQLite migrations and repositories
crates/bhippi-providers/       Local, CLI, and API model adapters
ui/                            React workspace and Three.js viewport
tests/fixtures/engine/         Deterministic engine and release fixtures
prompts/                       Runtime model-facing instructions
.github/assets/                Public README screenshots
```

## Quick start

### Prerequisites

| Requirement | Version or note |
| --- | --- |
| Rust | Stable 1.85 or newer |
| Node.js | 22 with npm |
| Desktop webview | Microsoft Edge WebView2 on Windows |
| Native tooling | Platform dependencies required by Tauri 2 |
| AI provider | Optional; install or configure at least one supported provider to use agent features |

See the official [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for platform-specific native dependencies.

### Clone, build, and run

```bash
git clone https://github.com/memegyanfactory-gif/BhippiADE.git
cd BhippiADE
npm ci --prefix ui
npm run build --prefix ui
cargo run -p bhippi-app --bin bhippi-desktop
```

Bhippi stores runtime configuration and local state outside the repository. Configure providers through the application. Secrets belong in the operating-system keychain and must never be committed to the project.

### Typical workflow

1. Add or open a project from the left sidebar.
2. Start an Agent session and select an available provider and model.
3. Use Editor for source-level inspection, Browser for research, or Engine for visual authoring.
4. Review agent changes before accepting them into the project.
5. Open the Engine pane for viewport-dependent inspection and play evidence.
6. Run `/gamedebug` for a structured game-quality report.
7. Run the quality gates before sharing or packaging a build.

## Quality and safety

Run the same core checks expected by CI:

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

The project follows several fail-closed rules:

- Agent edits cross the Rust transaction boundary and are journaled.
- Capability denial blocks an entire action batch before it writes anything.
- Play mode does not mutate the authored scene; stopping returns to the original document.
- Gameplay scripts run with deterministic step and call-depth limits.
- Imported or generated release assets require resolved licence metadata.
- Secrets are scrubbed from logs and replay data and stored only through the OS keychain.
- Safety, licence, accessibility, and release gates block instead of silently warning.
- Rust forbids unsafe code and denies `unwrap()` and `expect()` outside tests through workspace lint policy.

## Current status

| Available now | Still maturing |
| --- | --- |
| Desktop project workspace | Production packaging across every desktop target |
| Single and multi AI sessions | Broader live-provider and account compatibility |
| Integrated editor and browser | Production-grade rendering and large-world performance |
| Visual engine workbench | Full physics, animation, VFX, audio, and networking backends |
| Typed transactional engine actions | Real-device performance and compatibility evidence |
| Scene recovery and journals | End-to-end export certification for every target |
| `/gamedebug` reports and repair contracts | Expanded visual authoring tools and asset pipelines |

The repository contains substantial tested foundations, but it should not be presented as a finished replacement for a mature commercial engine yet. Claims about runtime, platform, or release readiness should be backed by real host evidence.

## Contributing

Contributions should preserve the Rust/TypeScript ownership boundary, fail closed when validation cannot prove safety, and include tests proportional to the risk of the change.

Start with [CONTRIBUTING.md](CONTRIBUTING.md), run the quality gates above, and keep pull requests focused. Bug reports should include the operating system, reproduction steps, expected result, actual result, and relevant logs with secrets removed.

## Security

Please do not open a public issue for a vulnerability. Follow [SECURITY.md](SECURITY.md) for private reporting and supported-version information.

## License

Bhippi is licensed under the GNU Affero General Public License v3.0 only (`AGPL-3.0-only`). See [LICENSE](LICENSE).
