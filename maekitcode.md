# Bhippi ADE conversion plan

**Task:** Turn Bhippi's desktop shell into a project-first agentic development environment (ADE), while preserving its technology/AI research engine and safety invariants.

**Status rule:** A box is checked only after the implementation exists and the relevant build/test or visual check has passed.

## 0. Product contract

- [x] Treat the supplied T3 Code / modern IDE screenshots as layout and interaction inspiration only; retain Bhippi branding, amber accent, and existing engine concepts.
- [x] Keep provider/model/effort routing honest: no invented connections, models, repositories, branches, or capabilities.
- [x] Preserve INV-032 generated IPC, INV-034 keyboard/focus/reduced-motion behavior, and Rust ownership of filesystem/process behavior.
- [x] Update `docs/04-PAGES.md`, `docs/PROGRESS.md`, and the accepted ADR so the shipped project-first shell is documented.

## 1. Fresh launch and project lifecycle

- [x] Add a typed Rust `ProjectSummary` contract with name, canonical path, repository state, and last-opened timestamp.
- [x] Persist known projects and the active project in `~/.bhippi/config.toml`; never infer a project from the process working directory.
- [x] Fresh install opens with no conversation, file, or previous project selected.
- [x] Build a clean first-run screen with three clear actions: open a local folder, clone a Git URL, or create a project.
- [x] Add a keyboard-friendly Add Project dialog with source search, local folder path, Git URL, and create-project forms.
- [x] Validate paths/names/URLs in Rust and return actionable errors; never delete a project directory when removing it from Bhippi.
- [x] Persist the selected project across restarts while keeping “New session” clean inside that project.

## 2. ADE workspace shell

- [x] Replace chat-first navigation language with project/session language.
- [x] Add a compact project header showing project name, repository/branch truth, and project actions.
- [x] Add a project-header switcher and project session list. (Database-level session/project foreign keys remain follow-on work.)
- [x] Keep Chat/Research/Automation/Library available as Bhippi workspaces, but frame Chat as the project agent rather than a generic chatbot.
- [x] For a new project session, center the composer in the empty workspace.
- [x] After the first submitted instruction, animate the composer to the bottom and keep it bottom-docked for that project session.
- [x] Restore existing sessions directly in the docked state.
- [x] Provide clear loading, empty, error, and populated states for projects and sessions.

## 3. External tools and source control

- [x] Add typed Rust commands to open the active project in VS Code, Cursor, Antigravity, and the system file explorer.
- [x] Detect tool availability and disable unavailable launch actions with a truthful explanation.
- [x] Add an “Open in…” drop-up in the project header and a first-run connection area.
- [x] Add `Initialize Git` for a non-repository project through an explicit argv process call.
- [x] Add Git clone as an explicit argv process call with progress/error messaging; do not execute shell strings.
- [x] Surface repository state in the UI without claiming provider authentication.
- [ ] Keep remote hosting/authentication work in Settings and label unsupported providers honestly.

## 4. Visual system and icons

- [x] Modernize the shell tokens: near-black canvas, restrained layered surfaces, amber/yellow active accent, consistent 6/8/12px radii, crisp hairlines, and quieter secondary text.
- [x] Replace legacy-looking glyphs with one coherent stroke icon family at consistent optical sizes. (Rewritten in ADR-0014: 24×24 box, 20×20 live area, 1.6px stroke, `currentColor` only, plus per-filetype glyphs.)
- [x] Create modern project, folder, Git, terminal, editor, branch, settings, and action icons without external brand copying.
- [x] Add selected, hover, pressed, disabled, focus-visible, and destructive states to every new control.
- [x] Keep density suitable for a desktop ADE at 1240×820 and scale down cleanly to narrow windows.
- [x] Respect `prefers-reduced-motion` and existing AA token contrast.

## 5. Composer, effort, and Ultra mode

- [x] Provider picker is separate from the model picker and persists the explicit choice.
- [x] Effort has four real backend levels and exposes click, drag, arrows, Home, and End.
- [x] Effort control uses a squarish rail/knob and a CSS particle lattice, with motion disabled for reduced-motion users.
- [x] Rename the highest presentation to “Ultra” consistently and apply the amber project theme.
- [x] When Ultra is selected, show an animated green contribution-block particle field inside the effort drop-up only; it must be decorative, bounded, and non-essential.
- [x] Keep the current effort name and behavior readable without color or motion.
- [x] Make the project name/path and true scope visible as composer context chips.

## 6. Settings and connections

- [ ] Rework Settings into the full left-rail set: General, Appearance, Providers, Integrations, Source Control, Connections, Usage, and About. (Appearance, Providers, Integrations, Usage, and product sections ship in this pass.)
- [x] Add System/Light/Dark scheme previews plus restrained Bhippi theme swatches.
- [ ] Add contrast, interface density, glass/surface opacity, interface font, monospace font, and word-wrap controls only where the runtime genuinely supports them.
- [ ] Add source-control detection rows for Git and remote providers with installed/authenticated/coming-soon states.
- [x] Add integration rows for external editors with truthful runtime availability; project launch actions live in the project header.
- [ ] Keep unavailable network/remote-environment features as explicit coming-soon states, not inert switches.

## 7. Verification and delivery

- [x] Regenerate `ui/src/lib/ipc.ts` after every command/type surface change.
- [x] Add Rust unit tests for project validation, accepted Git transports, and persistence round trips. (A direct command-level no-delete test remains open.)
- [x] Run `npm run build` in `ui`.
- [x] Run focused Rust tests and `cargo test -p bhippi-types --test architecture`.
- [x] Run `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] Launch the desktop build, inspect fresh-launch, active-project, existing-session, tool drop-up, Settings, and Ultra states at 1240×820.
- [ ] Verify keyboard-only project onboarding, composer submission, tool menu, effort slider, and Settings navigation.
- [ ] Record any unavailable locally installed editor/remote authentication as a limitation rather than marking it complete.

## Implementation order

1. Rust project/tool contract and tests.
2. Generated IPC and thin API wrapper.
3. Project gate, onboarding dialog, and active-project header.
4. Project-scoped ADE shell and composer state transition.
5. Icons, tokens, Ultra particles, and responsive polish.
6. Settings/integration surfaces.
7. Docs, builds, tests, desktop visual QA, then final checklist update.

## 8. Project isolation correction

- [x] Keep the full sidebar visible on the no-project starting screen, with project actions enabled and project-only navigation disabled until selection.
- [x] Replace path-only local-folder onboarding with the native operating-system directory picker; keep a manual path fallback for accessibility and recovery.
- [x] Use the native picker for the parent directory in Create Project and Clone Project flows.
- [x] Add `project_path` to every conversation record and return only conversations belonging to the active project.
- [x] Require the active project path for create, fetch, delete, send, and regenerate commands; reject cross-project conversation ids in Rust.
- [x] Switch projects with a clean active chat selection while restoring that project's own session list.
- [x] Give every provider request a canonical workspace path and start CLI providers inside that exact directory instead of shared `~/.bhippi/workspace`.
- [x] Add a workspace boundary statement to the agent system context so non-CLI/local providers understand the selected project scope.
- [x] Canonicalize and validate workspace paths immediately before every provider call so a removed or replaced directory fails closed.
- [x] Add tests proving project A cannot list, read, delete, regenerate, or append to project B's conversations.
- [x] Document the boundary honestly: Bhippi-owned operations are path-confined and sessions are isolated; third-party coding CLIs are working-directory scoped but are not an OS security sandbox unless that provider supplies one.
- [x] Regenerate IPC, run UI build, Rust tests, architecture, fmt, clippy, and rebuild the embedded desktop executable.


## 9. In-app workbench, activity dock, and rules (ADR-0014)

- [x] Rust `files` module: lazy directory listing, file read/write with a 1 MB cap and a binary
      test, TCP-probed preview targets, and project rules — all confined to the active project
      resolved from Rust state.
- [x] Two-layer path confinement (pre-filesystem string rejection, then canonical `starts_with`),
      with tests for `..`, drive prefixes, and control characters.
- [x] Regenerated IPC; thin `api.ts` wrappers added.
- [x] Right-hand workbench pane, closed by default, width and mode remembered, resizable splitter,
      `Ctrl/Cmd+B` and `Ctrl/Cmd+'`.
- [x] Editor: lazy tree with per-filetype glyphs, tab, dirty dot, gutter, breadcrumb, `Ctrl/Cmd+S`,
      in-house per-line tokenizer returning spans (never `innerHTML`).
- [x] Browser: loopback origins only, probed port list, honest idle labels, pane-local history,
      non-local URLs handed to the system browser.
- [x] Editor ⇄ Browser switch: one sliding pill with an overshoot curve, `role="radiogroup"`,
      arrow keys, reduced-motion safe.
- [x] Activity dock above the composer: breathing current step, expandable list of every emitted
      engine step with icons/state/elapsed, and the agent's permission questions inline.
- [x] Project rules in `.bhippi/rules.md`, wired into the system prompt via `prompts/chat-rules.md`
      after the workspace boundary and before the effort directive.
- [x] Project identity moved into the sidebar with a hard 20-character name rule (`clipName`).
- [x] Chat motion pass: directional turn entry, composer focus lift, sprung send button, dealt
      suggestion chips, tool-row settle, breathing caret.
- [x] `npm run build`, `cargo fmt --check`, `cargo clippy -p bhippi-app -D warnings`,
      `cargo test -p bhippi-app`, `cargo test -p bhippi-types --test architecture`, desktop binary
      rebuilt.
- [ ] Native 1240×820 visual and keyboard-only QA of the workbench, both modes, the splitter, and
      the dock.
