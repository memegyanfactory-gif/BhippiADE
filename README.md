<p align="center">
  <img src="ui/public/bhippi-logo.png" width="104" alt="Bhippi logo" />
</p>

<h1 align="center">Bhippi ADE</h1>

<p align="center">
  <strong>Build playable 3D games with AI agents and a live Godot 4 engine in one local-first desktop studio.</strong>
</p>

<p align="center">
  <a href="https://github.com/memegyanfactory-gif/BhippiADE/actions/workflows/ci.yml"><img alt="CI status" src="https://github.com/memegyanfactory-gif/BhippiADE/actions/workflows/ci.yml/badge.svg" /></a>
  <img alt="Rust 1.85 or newer" src="https://img.shields.io/badge/Rust-1.85%2B-CE412B?logo=rust" />
  <img alt="Godot 4" src="https://img.shields.io/badge/Engine-Godot%204-478CBF?logo=godotengine&logoColor=white" />
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
  <img src=".github/assets/bhippi-ade-workbench.png?raw=true" width="100%" alt="Bhippi ADE Studio with AI chat and live embedded Godot 4 3D engine viewport" />
</p>

<p align="center"><em>One unified desktop game studio for AI collaboration, live Godot 4 viewport authoring, code editing, web research, play inspection, and version recovery.</em></p>

> [!IMPORTANT]
> Bhippi is under active development. Windows is the primary desktop target today. Core Rust validation also runs on Windows, macOS, and Linux. The engine runtime is Godot 4.

## Overview

Bhippi ADE is a local-first, AI-native game development studio built with Rust, Tauri, React, and Godot 4. You describe a game, Bhippi plans it, builds it in a real Godot 4 project, plays it, and iterates — every change typed, journaled, undoable, and measured. The engine is Godot; Bhippi is the studio around it.

The central idea is simple: **humans and AI agents should use the same safe engine boundary**. Reads are explicit, writes are typed actions (GDScript check-compiles before writing), capability policy is enforced before mutation, and authored project data remains recoverable.

| Surface | What it provides |
| --- | --- |
| **Studio & Viewport** | Live embedded Godot 4 3D engine viewport with perspective navigation, 3D grid, transport controls, and docked drawers (Assets, Library, Code, Console, Versions). |
| **Agent workspace** | Project-scoped conversations, parallel multi-agent sessions, provider and model selection, live token spend meters, and reviewable changes. |
| **Project Editor** | Project explorer tree and source editor for Godot scene files (`.tscn`), GDScript (`.gd`), `project.godot`, and manifests (`Bhippi.game.toml`). |
| **Browser** | Integrated research surface with tabs and bookmarks for web documentation, asset stores, and API references beside active projects. |
| **Rust authority** | Typed actions, transaction validation, Godot process supervision, journaled checkpoints, telemetry probes, and release-blocking safety gates. |

## Product tour

### 1. Unified AI Game Studio

Bhippi bridges the reasoning of frontier AI models with a live Godot 4 engine inside one local-first desktop application. Instead of jumping between terminal windows, external AI web chats, code editors, and engine viewports, creators and AI agents collaborate across a unified, synchronized studio.

<p align="center">
  <img src=".github/assets/bhippi-ade-workbench.png?raw=true" width="100%" alt="Bhippi ADE Studio with AI chat and live embedded Godot 4 3D engine viewport" />
</p>

*The Studio in action: Conversational AI prompt interface with quick suggestion chips on the left, alongside the live embedded Godot 4 3D viewport, camera perspective controls, transform toolbar, transport controls (`Play`, `Playtest`, `Watch play`), and bottom drawer panels (`Assets`, `Library`, `Code`, `Console`, `Versions`).*

---

### 2. Live Agent-Engine Collaboration & Scene Editing

AI agents don't work in the dark. In Bhippi ADE, an agent can reason about game mechanics, inspect level parameters, and author scenes and scripts while the code editor or engine viewport remains active beside it.

<p align="center">
  <img src=".github/assets/agent-workspace.png?raw=true" width="100%" alt="AI agent session inspecting game architecture beside the code editor showing main.tscn" />
</p>

- **Live Context Feedback**: The agent inspects scene files and scripts in response to creator prompts while the creator visually verifies the corresponding scene elements and code.
- **Direct Scene Inspection**: Full syntax-highlighted review of Godot scene files (`main.tscn`), node hierarchies (`Node3D`, `DirectionalLight3D`, `Camera3D`, `CapsuleMesh`, `CollisionShape3D`), procedural skies, and environment lighting.
- **Zero App Switching**: Viewport inspections, asset browsing, code editing, and agent prompts occur within a single window, eliminating friction between thinking and testing.

---

### 3. Parallel multi-agent operations

Multi mode turns the workspace into a concurrent agent operations center. Multiple AI providers and models can analyze different aspects of the same Godot project simultaneously, explore files, execute commands, and prepare changes in parallel without cross-contaminating session state.

<p align="center">
  <img src=".github/assets/multi-agent-workspace.png?raw=true" width="100%" alt="Parallel AI agent sessions inspecting and authoring a Godot game project side by side" />
</p>

- **Side-by-Side Execution**: Run multiple independent agent sessions (e.g. Claude Code, Fable 5, big-pickle, Grok, OpenCode, GPT-5) side by side with full drag-and-drop reordering.
- **Deep Codebase Inspection**: Agents autonomously read game manifests (`Bhippi.game.toml`), parse Godot scenes (`scenes/main.tscn`), explore gameplay scripts (`scripts/`), and verify autoload telemetry (`BhippiProbe`).
- **Live Status & Telemetry**: Clear visual status indicators (`Running`, `Idle`), real-time execution timers, step tracking, and token usage meters.
- **Safe Change Reviews**: Non-destructive diff tracking with dedicated `Review Changes` panels before any mutation touches disk.

---

### 4. Full-featured source and manifest editor

Behind the visual tooling lies a high-performance code and project editor tailored for Godot 4 architectures. Creators can inspect and refine raw scene definitions, GDScript files, shaders, and declarative game manifests.

<p align="center">
  <img src=".github/assets/project-editor.png?raw=true" width="100%" alt="Bhippi integrated project editor showing Bhippi.game.toml and project explorer" />
</p>

- **Project Explorer Tree**: Full visibility into authored project files, engine caches (`.godot`), addons, runtime scaffolding (`bhippi`), scenes, scripts, and Godot project files.
- **Declarative Project Manifests**: Direct editing of `Bhippi.game.toml` with pinned Godot version (`4.7.1`), main scene, telemetry probe activation, 3D render pipeline settings (MSAA, physics gravity), and multi-platform build targets (Windows, Android, iOS).
- **Multi-Document Tabs**: Rapid tab switching between scenes (`.tscn`), GDScript files (`.gd`), manifests (`.toml`), and export presets with line numbers and encoding stats.

---

### 5. Integrated research browser

Research is a first-class citizen in Bhippi ADE. The integrated browser surface keeps web documentation, Godot API references, shader tutorials, and online asset libraries right inside the developer's workspace.

<p align="center">
  <img src=".github/assets/integrated-browser.png?raw=true" width="100%" alt="Integrated web browser surface inside Bhippi ADE" />
</p>

- **In-Context Research**: Open web pages and documentation directly beside your project without switching to an external browser.
- **Modern Browser Controls**: Dedicated address bar, tab management, navigation controls (back, forward, reload, home), and one-click access to Google, Wikipedia, GitHub, and YouTube.
- **Fast Surface Toggling**: Switch between `Editor`, `Browser`, and `Engine` with a single click in the top tab bar.

---

### 6. Compact project navigation shell

A persistent, responsive sidebar keeps navigation effortless across projects, tools, and background agent tasks.

<p align="center">
  <img src=".github/assets/project-navigation.png?raw=true" width="260" alt="Compact sidebar navigation showing active project sessions and tool shortcuts" />
</p>

- **Fast Tool Docking**: Instant access to Engine, Projects, Games, Assets, and Add-ons.
- **Live Project Status**: Real-time indicators showing active concurrent sessions (`3 active`), project pinning, and session window thumbnails.

## Capabilities

### AI-native workspace

- Project-scoped chat and CLI sessions with independent drafts and live state.
- Single-session focus and multi-session organization with bidirectional drag-and-drop.
- Provider discovery and selection for installed or configured Claude, Codex, Grok, Kimi, OpenCode, custom, and local-model routes.
- Streaming output, visible limit meters in normal mode, typed provider faults, cancellation, and change review.
- Imported skills and bounded computer-use controls for supported vision-capable providers.
- No silent provider fallback: unavailable and disconnected states are represented truthfully.

### Godot 4 engine integration

- Live embedded Godot 4 viewport with perspective navigation, world grid, and transport controls (`Play`, `Playtest`, `Watch play`).
- Docked drawers for Assets, Capability Library, Code, Console, Versions, Debugger, Audio, Animation, and Shader Editor.
- Built-in `BhippiProbe` autoload for scripted input injection and playtest telemetry.
- Versioned checkpoints with one-click create and revert via the journal.
- Typed transactional actions: GDScript is check-compiled before writing; `.tscn` and `project.godot` are never hand-written raw.

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
+-------------------------------------------------------------------------+
| React workspace                                                         |
| Studio * Multi-Agent Canvas * Project Editor * Browser * Godot Viewport |
+------------------------------------+------------------------------------+
                                     | generated, typed Tauri IPC
                                     v
+-------------------------------------------------------------------------+
| bhippi-app                                                              |
| Desktop integration * sessions * permissions * Godot process supervision|
+---------------+--------------------+--------------------+---------------+
                |                    |                    |
                v                    v                    v
+------------------------+ +------------------+ +------------------------+
| bhippi-engine          | | bhippi-core      | | bhippi-providers       |
| Godot bridge & probe   | | Orchestration    | | Local/CLI/API routes   |
| Typed transactions     | | Budgets/events   | | Streaming and usage    |
| Capability registry    | | Cancellation     | | Typed faults           |
| Versions & checkpoints | | Context routing  | | Account state          |
+-----------+------------+ +------------------+ +------------------------+
            |
            v
+-------------------------------------------------------------------------+
| bhippi-db * SQLite repositories, journals, recovery, and metadata       |
+-------------------------------------------------------------------------+
```

Business rules live in Rust. TypeScript renders state, hosts the isolated viewport and play presentation layer, and sends typed requests through generated IPC bindings. SQL remains inside `bhippi-db`; provider secrets remain outside the repository.

### Repository layout

```text
crates/bhippi-app/             Tauri desktop shell and application seams
crates/bhippi-engine/          Headless engine domain and Godot integration authority
crates/bhippi-engine-build/    Build preflight, export checks, and release gates
crates/bhippi-engine-viewport/ Viewport protocol and presentation contracts
crates/bhippi-core/            Orchestration, context, budgets, and cancellation
crates/bhippi-db/              SQLite migrations and repositories
crates/bhippi-providers/       Local, CLI, and API model adapters
ui/                            React workspace and live Godot 4 viewport
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
3. Use Editor for source-level inspection, Browser for research, or Studio for live Godot authoring.
4. Review agent changes before accepting them into the project.
5. Open the Studio viewport for real-time inspection, playtesting, and telemetry.
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
- GDScript changes are check-compiled before writing; `.tscn` and `project.godot` are never written raw.
- Imported or generated release assets require resolved licence metadata.
- Secrets are scrubbed from logs and replay data and stored only through the OS keychain.
- Safety, licence, accessibility, and release gates block instead of silently warning.
- Rust forbids unsafe code and denies `unwrap()` and `expect()` outside tests through workspace lint policy.

## Current status

| Available now | Still maturing |
| --- | --- |
| Desktop project workspace | Production packaging across every desktop target |
| Single and multi AI sessions | Broader live-provider and account compatibility |
| Integrated editor and browser | High-density 3D level procedural authoring |
| Live Godot 4 viewport & transport | Automated playtest agent action generation |
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
