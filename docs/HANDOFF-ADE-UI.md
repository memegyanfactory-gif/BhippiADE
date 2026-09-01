# Handoff — ADE shell, workbench, and activity surface

**Purpose:** hand this work to another agent (or another session) without losing context.
**Started from:** `maekitcode.md` (the previous ADE conversion plan, mostly complete).
**Owner's request, verbatim intent:**

1. The side panel must show the open **project's name**.
2. Better **icons** across the app — the existing ones were weak.
3. A **rule** that a project name is never rendered past a fixed length, so a long folder
   name cannot blow up the chrome.
4. **Full UI pass** — clean, professional.
5. **Chat improvements**, including animations, and **rules** for the agent.
6. An **in-app browser** so anything the agent builds can be run inside Bhippi. Hidden by
   default; switchable from the top.
7. An **editor** like VS Code — real folder tree, real file icons, a coding surface.
8. A **satisfying Editor ⇄ Browser toggle**.
9. *(added mid-session)* An **activity drop-up** in the composer showing what the agent is
   doing right now — one row per step, breathing text, proper icons — expandable, and the
   place where the agent asks the user when it lacks information.

Authority order still applies: spec → invariants → architecture → module contracts →
data/pages/pipelines → newest ADR → code comments.

---

## Status legend

- `[x]` implemented **and** the relevant build/test passed
- `[~]` implemented, not yet verified by a build or a visual check
- `[ ]` not started

---

## 1. Rust backend — workbench filesystem and preview

New file: `crates/bhippi-app/src/files.rs`.

- [x] `WorkspaceEntry`, `WorkspaceFile`, `PreviewTarget`, `ProjectRules` typed for specta.
- [x] `list_workspace_dir(relative)` — lazy per-directory listing, directories first then
      files A–Z, skips `node_modules`, `target`, `.git`, `dist`, `build`, `.next`,
      `.svelte-kit`, `.turbo`, `__pycache__`, `.venv`. Dotfiles stay visible.
- [x] `read_workspace_file(relative)` — 1 MB cap, NUL-byte binary test, returns
      `editable: false` with a reason rather than an error for both cases.
- [x] `write_workspace_file(relative, text)` — refuses directories and oversized files.
- [x] `preview_targets()` — probes nine common dev-server ports over TCP with a 180 ms
      timeout, reads `package.json` only to *label* an idle port with the command that
      would start it. **A target is `reachable` only if a TCP connect actually
      succeeded.** Nothing is invented.
- [x] `read_project_rules()` / `write_project_rules(text)` — `.bhippi/rules.md` inside the
      project folder. A missing file is the normal first-run state, not an error.
- [x] Path confinement: `sanitize_relative` rejects `..`, drive prefixes, and control
      characters *before* the filesystem is touched; `resolve` then canonicalizes and
      checks `starts_with(project_root)`, so a symlink pointing outside fails too.
- [x] The active project comes from **Rust state** (`required_project_path`), never from a
      frontend-supplied path. `required_project_path` was widened to `pub(crate)` in
      `commands.rs` for this.
- [x] Unit tests: traversal refused, drive prefix refused, control characters refused,
      normal paths normalised, forward-slash display, extension lowercasing.

## 2. Agent rules wired into the prompt

- [x] `prompts/chat-rules.md` created with a `version: 1` header (INV-035).
- [x] `crates/bhippi-app/src/chat.rs`: `project_rules_block()` loads `.bhippi/rules.md`
      from the turn's workspace, truncates at 8 000 characters, and returns `None` for a
      missing or blank file.
- [x] The block is inserted **after** the workspace boundary statement and **before** the
      effort directive, and the prompt states plainly that rules never widen access or
      override the workspace boundary, the technology/AI scope, or any safety rule.

## 3. IPC

- [x] All six new commands registered in `ipc_builder()` in `lib.rs`.
- [x] `ui/src/lib/ipc.ts` regenerated via `cargo run -p bhippi-app --bin export-bindings`
      (INV-032 — never hand-edit that file).
- [x] Thin wrappers added to `ui/src/lib/api.ts`: `workspaceDir`, `readFile`, `writeFile`,
      `previewTargets`, `projectRules`, `saveProjectRules`.

## 4. Icons

- [x] `ui/src/components/icons.tsx` rewritten as one family: 24×24 box, 20×20 live area,
      1.6px stroke, round caps/joins, `currentColor` only, half-pixel grid at 16px.
- [x] New glyphs: `IconEditor`, `IconBrowser`, `IconReload`, `IconSave`, `IconRules`,
      `IconSplit`, `IconPanelRight`, `IconChevronRight`, `IconFolderOpen`, `IconSearch`,
      `IconDot`, plus file glyphs (`IconFileCode/Text/Data/Style/Image/Config`).
- [x] `FileGlyph` maps **filename** (not just extension) → glyph + desaturated tint, so
      `package.json` and `Cargo.toml` are recognised as themselves. Tints are muted on
      purpose — a saturated logo column beside the amber accent reads as confetti.
- [x] `IconSearchWeb` kept as an alias of `IconSearch` so research surfaces keep compiling.

## 5. Project name rule

- [x] `ui/src/lib/format.ts`: `MAX_NAME_CHARS = 20`, `clipName()`, `clipPath()`, `bytes()`.
- [x] **Why it is not CSS:** an ellipsis still lets the element request its full width
      first, which is what let a long folder name set the width of the whole rail. The
      string is cut in JS; the full value stays in `title`.
- [x] Applied in the sidebar badge, the switcher menu, the header path, the composer context
      chip, and the first-run recent-projects list.

## 6. Sidebar

- [x] Rewritten `ui/src/chrome/Sidebar.tsx`. Project identity is now the **first thing in
      the rail**: name + branch (or clipped path), with the project switcher menu moved
      here from the header.
- [x] Props changed: `projectActive: boolean` → `project: ProjectSummary | null`, plus
      `projects`, `onSelectProject`. `App.tsx` updated to match.
- [x] "Project sessions" heading shortened to "Sessions" (the project name is directly
      above it now).

## 7. Workbench — editor and browser

New folder `ui/src/workbench/`:

- [x] `highlight.ts` — per-line tokenizer, no new dependencies. Returns **spans, never an
      HTML string**, so a file containing `<script>` is text. Grammars: JS family, Rust,
      Python, CSS, data (json/toml/yaml), shell. Block-comment state carries across lines.
- [x] `FileTree.tsx` — lazy expansion, per-directory cache, refresh token invalidates
      everything on project switch.
- [x] `CodeView.tsx` — highlighted `<pre>` behind a transparent `<textarea>`. Line gutter,
      breadcrumb, tab, dirty dot, `Ctrl/Cmd+S`, Tab inserts two spaces. Read-only states
      for binary and oversized files explain *why*.
- [x] `BrowserView.tsx` — **loopback only** (`localhost`, `127.0.0.1`, `0.0.0.0`, `::1`).
      Anything else is refused with a notice and offered to the system browser instead.
      History is kept locally because a cross-origin frame will not report its navigation
      and drawing back/forward from guesses would lie.
- [x] `ModeSwitch.tsx` — one sliding pill (not two cross-fading backgrounds), overshoot
      curve `cubic-bezier(0.34, 1.42, 0.5, 1)`, `role="radiogroup"`, arrow keys.
- [x] `Workbench.tsx` — both panes stay mounted once opened (unmounting the browser would
      reload the dev server on every toggle; unmounting the editor would drop a buffer).
      The browser pane does not mount at all until first requested — no idle port probing.
- [x] `ui/src/styles/workbench.css` — all of the above.
- [x] `App.tsx` — resizable splitter, width clamped to 28–72 % and persisted, pointer
      listeners on `window` (a fast drag outruns a 5px handle), iframe made
      `pointer-events: none` while dragging.
- [x] Hidden by default on every launch. `Ctrl/Cmd+B` toggles, `Ctrl/Cmd+'` flips mode.
- [x] Mode state lives in `App.tsx` and is passed down, so the toolbar glyph and the switch
      can never disagree.

## 8. Project header

- [x] Rewritten as a **toolbar**: workspace-lock fact, clipped path, branch truth, then
      Rules · Open in… · workbench toggle. Identity moved out to the sidebar.
- [x] External tools get distinct glyphs instead of four identical rows.

## 9. Rules panel

- [x] `ui/src/screens/RulesPanel.tsx` — modal editing `.bhippi/rules.md`. Stored **in the
      project folder** so rules travel with the repo and switching projects switches rules.

---

## 10. Activity dock — DONE

- [x] `ui/src/screens/ActivityDock.tsx` + `ui/src/styles/activity.css`.
- [x] Collapsed trigger: equaliser pulse, breathing current step, summary ("3 steps running",
      "thinking", "needs your answer").
- [x] Drop-up lists every `ToolActivity` for the running turn — action glyph, state word,
      elapsed time. Running rows breathe (opacity only, 2.6 s); settled rows stop.
- [x] `PermissionRequest` routed into the same panel with action, scope, detail, risk chip, and
      Allow / Deny inline. A question opens the panel itself.
- [x] Reduced-motion block: pulse bars settle at full height, breathing text at full opacity.
- [x] **Honesty held.** Every row comes from an emitted event (`chat-tool`, `chat-thinking`,
      `chat-permission-requested`). No invented "background agents" — the shared screenshot was
      used as layout inspiration only, since Bhippi does not run parallel agents today.

## 11. Chat motion pass — DONE

- [x] Directional turn entry (user from the right, agent from the left), composer focus lift,
      sprung send button, dealt suggestion chips, tool-row settle flash, breathing caret,
      dropped scroll pill. All collapse under `prefers-reduced-motion`.

## 12. Loose ends — DONE

- [x] `workbench.css` and `activity.css` imported in `ui/src/main.tsx`.
- [x] `clipName` applied to the composer context chip.
- [x] Dead `.project-identity` / `.project-folder` rules removed from `app.css`; `.side-project`
      and `.project-badge` styles written.
- [x] `.project-switch-menu` re-anchored to the sidebar badge (`top: calc(100% + 5px); left: 0`).
- [x] `ProjectStart` recent list and tool row given the new glyphs and the name rule.

## 13. Verification — ALL GREEN

- [x] `cargo build -p bhippi-app --bin export-bindings` + bindings regenerated
- [x] `npm run build --prefix ui` (tsc + vite) — clean
- [x] `cargo fmt --all -- --check` — clean
- [x] `cargo clippy -p bhippi-app --all-targets -- -D warnings` — clean
- [x] `cargo test -p bhippi-app` — 23 passed, including the five new `files::tests`
- [x] `cargo test -p bhippi-types --test architecture` — 2 passed
- [x] `cargo build -p bhippi-app --bin bhippi-desktop` — desktop binary rebuilt with the new UI
- [ ] **Not done: native visual QA.** Launch the desktop build at 1240×820 and check fresh
      launch, active project, workbench open/closed, both modes, the switch, the splitter, the
      activity dock during a real turn, and narrow-window behaviour.
- [ ] **Not done: keyboard-only pass.** Project switcher, workbench toggle (`Ctrl/Cmd+B`), mode
      switch (`Ctrl/Cmd+'` and arrow keys), splitter (arrow keys), tree, editor save
      (`Ctrl/Cmd+S`), rules dialog, activity dock (Escape).

## 14. Documentation — DONE

- [x] `docs/adr/0014-in-app-workbench-and-activity-dock.md` written and accepted.
- [x] `docs/04-PAGES.md` — A-1 amended, A0.4 amended, new A1c (workbench), A1d (activity dock),
      A1e (project rules).
- [x] `docs/PROGRESS.md` — session log row added.
- [x] `maekitcode.md` — §4 icon row checked, new §9 added.

---

## What is genuinely left

0. **A general "the agent needs more information" channel does not exist in the backend.**
   The dock routes the one ask mechanism the engine actually has today —
   `chat-permission-requested`, a consequential-action approval — into the panel, with Allow /
   Deny inline. A clarifying *question* ("which of these two files did you mean?") has no event,
   no type, and no reply path in `chat.rs`, so none was faked in the UI. Building it means: a
   `ClarificationRequest` type beside `PermissionRequest`, an event, a `respond_clarification`
   command taking free text, and provider-side prompting that actually emits it. The dock's ask
   block is already shaped to host it — it needs a text input instead of two buttons.

1. **Native visual and keyboard QA** (the two unchecked boxes above). Everything else in this
   document has been implemented and verified by a build or a test.
2. **Durable `chat_turns` with a project foreign key** in `bhippi-db` — carried over from the
   previous session, still the right next structural piece.
3. `maekitcode.md` §3 and §6 remainders: remote hosting/authentication in Settings, the full
   Settings left rail, contrast/density/font controls, source-control detection rows.

---

## Invariants this work must not break

- **INV-032** — `ui/src/lib/ipc.ts` is generated. Regenerate after every command or type
  change; never hand-edit.
- **INV-034** — keyboard everywhere, visible focus, AA contrast, `prefers-reduced-motion`
  respected, no colour-only meaning.
- **INV-035** — prompts are files under `prompts/` with a `version:` header.
- **INV-036** — no `unwrap()` / `expect()` outside `#[cfg(test)]`.
- **R3** — filesystem and process behaviour stays in Rust; no business logic in TypeScript.
- **ADR-0013** — project-scoped workspaces. Every path is confined to the active project,
  resolved from Rust state.
- Technology/AI scope, robots and paywalls, no unlicensed images, no SQL outside
  `bhippi-db`, gates block rather than warn.
