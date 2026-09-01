# Bhippi — Pages & Screens
**Doc:** `04-PAGES.md` · **Derives from:** spec §18, §19, §14.7 · **Status:** authoritative
**Owner:** Frontend · **Rule:** the webview renders and takes input. It computes nothing.

Two page sets:
**A. The desktop app** — persistent chrome + 4 screens + a 7-tab settings modal.
**B. The generated blog** — the site the engine publishes.

Every page below specifies: purpose · regions · data in (IPC command / event) · the four
states (loading, empty, error, populated) · interactions · acceptance. A page is not done
until all four states exist.

---

# PART A — Desktop app

## A-1 · Project gate and ADE workspace (ADR-0012)

The desktop app is project-first. On a fresh install the shell opens no conversation, file, or
inferred working directory. The main area asks the user to **open a local folder**, **clone a Git
URL**, or **create a project**. Known projects may be reopened from a compact recent list.

The persistent sidebar is present before selection as well as after it. Before selection its
project navigation is disabled and the rail reads "Choose a project to begin". After selection,
the workspace renders the sidebar plus a 46 px project toolbar. **New project** is always at the
top of the rail; there is no per-session "New session" control — sessions start from a project
card's `+` menu (chat or CLI).

**Project identity lives in the project cards** (ADR-0014): folder mark, clipped name, and branch
(or a clipped path when the project is not a repository) sit at the top of each card, and clicking
a card head switches to that project. There is no project badge dropdown at the top of the rail.
Names are cut to **20 characters in JavaScript**, not by a CSS ellipsis — an ellipsised element
still requests its full width first, which is what let a long folder name set the width of the
whole rail. The full name and path stay in the `title`.

The toolbar therefore carries only working-state truth and actions: the workspace-lock statement,
the clipped canonical path, real Git/branch state, **Rules**, an **Open in** menu for detected
VS Code, Cursor, Antigravity, and the platform file manager, and the **workbench toggle**. Unavailable tools
stay disabled with an installation/PATH hint. Non-repositories expose **Initialize Git**. Every
filesystem and process operation is Rust-owned and uses generated IPC; forgetting a project only
removes its config reference and never deletes its directory.

Agent sessions are project-framed. A blank session puts the composer in the workspace centre;
submitting the first instruction docks it to the bottom and it remains there for that session.
The sidebar label is **Agent**, not Chat, and its history section is **Projects** — one card per
project, each card's own session icons sitting directly under that project's head, so the
project name above them is never repeated per session.
Conversation create/list/read/delete/send/regenerate operations resolve the active canonical project
inside Rust and reject cross-project ids. Provider CLIs start in that exact project directory.

Local folder and parent-folder controls open the native operating-system directory selector. Manual
path entry remains available as an explicit fallback.

States: *loading* — hairline progress only; *empty* — the three project source actions; *error* —
the exact path/Git/tool failure and repair hint; *populated* — project header, workspace, and
project session history. Keyboard focus, Escape-to-close, and reduced motion follow INV-034.

## Design contract (applies to every screen)

**Instrument, not dashboard.** Dark, quiet, high-density. The only thing that moves is the
thing that is actually happening — which is why the mind map building itself reads as
meaningful rather than decorative.

**Tokens** (`ui/src/styles/tokens.css`, single source — accent set by ADR-0009):

```
--bg #100F0D   --surface #171614   --surface-2 #1E1C19  --surface-3 #262320
--line #2C2926 --line-strong #3B3733
--text #EAE7E1 --text-dim #9A938A  --text-faint #6B655D
--accent #F0A02C  (live/active only)  --accent-warm #FF8B3D  --on-accent #1A1206
--warn #E3B341    --error #F85149     --ok #3FB950
--radius 4px (6px modals, 8px popovers)   --space 4/8/12/16/24/32/48
--fs 10/11/12/13/15/18/24
```

**Budget gauge ramp** — a *separate* scale from the accent, meaning "how much of the budget
is spent" and nothing else. It fills as the budget goes: `--gauge-0 #3FB950` under 50 %,
`--gauge-1 #E3B341` to 75 %, `--gauge-2 #F0883E` to 90 %, `--gauge-3 #F85149` above it, over
`--gauge-track #2C2926`. An empty track means nothing spent; a full red ring means the cap is
gone; no ceiling set means the track stays empty rather than reading as full. Per INV-034 the
ramp never carries meaning alone — every ring, bar and row prints its percentage as text.

**Type:** Inter/system 13 px base, 1.5 line-height; JetBrains Mono 12 px for data, URLs, IDs;
headings 15/18/22 px at weight 500 — never 700, never uppercase except 10 px tracked
eyebrows. **Exactly two type sizes per screen region.**

**Motion:** 120 ms ease-out for state, 200 ms for panels, spring only for mind-map node
entry. `prefers-reduced-motion` respected everywhere. No skeleton shimmer — a 1 px hairline
progress rail instead.

**Surfaces:** hairlines, never shadows. No gradients. No card decoration.

---

## A0 · Persistent chrome

```
┌ title bar · 40px ────────────────────────────────────────────────────────┐
│ ● bhippi                                                        ⚙  ─ □ ✕ │
├ sidebar · ~280px ─┬───────────────────────────────────────────────────────┤
│ ◧ ⌕      ← →     │                                                       │
│ [ + New project ]│                                                       │
│ Agent            │                                                       │
│ Research         │                                                       │
│ Automation       │                                                       │
│ Library          │                                                       │
│ PROJECTS  Single/Multi                                                   │
│  · …             │                                                       │
│ ─────────────    │                                                       │
│ ● bhippi v0.1.0  │                                                       │
├ status bar · 28px ───────────────────────────────────────────────────────┤
│ ollama:qwen2.5 · queue 2 · ● running X6 · ⏻ kill              ◔ gauge 4% │
└──────────────────────────────────────────────────────────────────────────┘
```

Navigation and the conversation list live in the **sidebar** (ADR-0010); there is no top tab
bar. The ticker is no longer persistent chrome — it moved into the Research screen (§A0.1).

### A0.1 Ticker strip (spec §15.3) — now in the Research screen

Moved out of persistent chrome by ADR-0010: it renders stories that exist to be researched,
so it lives above the Research pane, outside its scroll flow. Everything else is unchanged:

- Left: pulsing dot + `LIVE` when polling is healthy; **amber** when any feed is failing.
- Items: `[category] headline · source · relative time`, scrolling at 40 px/s.
- **Pauses on hover and on keyboard focus.** With `prefers-reduced-motion`, it becomes a
  static rotating list, not a scroller.
- Priority ≥ 78 renders in `--accent` and **does not scroll past until seen once**.
- Click → popover: headline · cluster members with outlets · primary source if detected ·
  buttons **Research now** (opens Research pre-filled with the tier picker) · **Watch topic**
  · **Ignore** (suppresses the cluster 72 h).
- Right: pause toggle + counter of today's auto-triggered sessions.
- Data: `ticker_stream()` event channel · `ticker_trigger(event_id, tier)`.
- States: *loading* — dot grey, "connecting". *empty* — "No qualifying stories yet." *error*
  — amber dot + "3 feeds failing — open Settings › Ticker." *populated* — as above.

### A0.2 Title bar
A slim draggable strip: the supplied dimensional Bhippi logo at the far left, tilted slightly and
allowed to break the strip edge without a surrounding tile · restrained wordmark (+ `demo` badge) ·
settings gear with a badge when skills
await approval · window controls. No tabs, no session pill — navigation is the sidebar's job
and running state reads in the status bar beside the provider.

### A0.3 Status bar
Active provider(s) for the current task · queue depth · the engine's **running** indicator
(`● running <phase>`), which moved here from the old title bar · **kill switch** (also a
global shortcut) · the **usage gauge**, far right. Errors persist here until dismissed —
never a toast that vanishes before it is read.

**Usage instruments (ADR-0009, amended by ADR-0021).** The status-bar ring is explicitly
Bhippi's local accounting guard: the active provider's recorded tokens against the configured
local cap, with local-midnight reset. The composer's plan meter is separate and reads only the
signed-in vendor account's own rolling windows. It carries provider, account identity, plan,
weekly percentage/reset, the short window when exposed, and a manual refresh. A provider that
does not expose a numerical allowance renders **Not reported** with the reason; the local cap is
never substituted. Escape and click-away close either drop-up without stealing composer focus.

### A0.4 Sidebar (ADR-0010, amended by ADR-0014)
~280 px, `--surface`, hairline right border; collapsing leaves a 48 px icon rail.
The rail shows **every project**, one smoked-glass card each, so a glance reveals where
the agent is working and what is running (W4 §4.2). The active project's card is pinned
first with an accent rail; the rest are ordered by the engine then by persisted drag order,
with a stable pinned group at the top.
0. **New project** — full-width, quiet: hairline border, `--surface-2` on hover. Never a
   filled accent block. Opens a small menu with three ways in, all native:
   **Open a folder** (Windows folder picker, the picked folder becomes the active project),
   **Create a project** (the project dialog's create flow — the parent is chosen with the
   same picker), and **Clone from Git** (the clone flow, which also browses for the target).
   This replaces the old "New session" trigger: a session cannot exist outside a project, so
   the entry point adds a project first.
1. **Icon row** — panel collapse · session filter (`/` focuses) · back / forward through
   screen history, disabled at the ends of the stack.
2. **Nav rows** — Agent / Research / Automation / Library as icon + label; the active screen
   is marked by an inset 2 px accent rail and `aria-current="page"`.
3. **Projects** — micro uppercase section header with the `Single / Multi` workspace-mode
   switch beside the label. One `proj-card` per project:
   - **Workspace mode** — a compact `Single / Multi` switch. Single
     opens one session at a time; Multi keeps every chat and CLI belonging to the active project
     visible in the main workspace (ADR-0023).
   - **Card head** — folder mark · clipped name · branch (or clipped `project_path` for
     non-git) · an "N active" pill when sessions are running or paused. Clicking a card head
     **switches to that project** (the screen's session scope follows it, and a blank session
     in that project — or its "New chat in …" affordance — starts a chat scoped to it). A pin
     action moves the card into a stable top group and prevents drag displacement until
     unpinned. Unpinned cards remain vertically draggable and their order persists locally.
   - **Session chips** — up to 8 per card; each chip shows the last assistant turn's
     provider mark (`ProviderLogo`, keyed by catalogue id), a terminal glyph for CLI
     sessions, a chat glyph when the provider could not be resolved, and a status dot
     (running = `--ok` + pulse, paused = `--warn`, failed = `--error`, idle = `--line`).
     Hover reveals a two-click delete bin (armed = red). Tooltip reads
     `provider · title · status · relative time`. The active session gets an accent ring.
   - **Overflow** — more than 8 sessions collapse behind "Show all N", a footer toggle that
     switches the chips to a wrapped grid.
   - **Empty** — a card with no sessions reads "No sessions yet" with an inline `+` that
     starts that project's first chat.
   - All four list states per project: loading ("Loading…"), empty, error (nothing in the
     engine responds → empty rail), and populated at real volume.
4. **Account row** — hairline-separated bottom strip: glyph, app version (read from Tauri at
   runtime), `demo` badge when applicable.

---

## A1 · Research (default screen)

**Purpose:** start a run, watch it think, inspect its evidence.

```
┌ topic input ───────────────────────────────────────────────────┐
│  What should Bhippi research?                            [↵]   │
└────────────────────────────────────────────────────────────────┘
   ( X2 )  ( X6 )  ( X12 )  ( X24 )        ← segmented, X6 default
   12 expansions · ~60-90 sources · ~30 min · 2000-3000 words

┌ mind map canvas ──────────────────────────┐ ┌ inspector ───────┐
│         ○───●───○                         │ │ node: "MoE       │
│        ╱    │    ╲                        │ │ routing collapse"│
│       ●     ●     ○  ← frontier (dim)     │ │ 7 dots           │
│        ╲   ╱                              │ │ ─────────────────│
│         ● ●  ← contradiction edge (red)   │ │ • 3.2× fewer …   │
│                                           │ │   arxiv.org  T1  │
└───────────────────────────────────────────┘ └──────────────────┘
 stage rail: plan ▓▓ expand ▓▓▓▓▓░░ synth ░ facts ░ write ░ publish ░
 47 sources · 168 dots · 9 primary · 4 contradictions · 6:12 elapsed
```

**Regions**
1. **Topic input** — single line, `↵` starts. Ticker "Research now" prefills it.
2. **Tier chips** — segmented, X6 default. The line beneath restates the chosen tier's
   budget contract; hovering any chip previews its contract. *The user always knows what
   they are buying.*
3. **Mind map canvas** — nodes sized by dot count, ring = authority, fill = status
   (frontier / exploring / explored / pruned). Typed, labelled edges; contradiction edges
   render in `--error` **and dashed** (no colour-only meaning). Positions stream from Rust.
4. **Inspector** — the selected node's dots, each with source domain and tier badge,
   confidence, and observed date. Click a dot → the source's extracted text at the recorded
   offsets.
5. **Stage rail** — the FSM from `01-ARCHITECTURE §5`, current stage filling.
6. **Counter line** — sources · dots · primary · contradictions · elapsed, live.

**Data in:** `research_start(topic, tier, opts)` · `mindmap_get(id)` · `session_get(id)` ·
events `mindmap.delta`, `dot.added`, `source.fetched`, `session.stage_changed`.
**Data out:** `research_pause/resume/cancel(id)` · `research_focus_node(id, node_id)` ·
`mindmap_export(id, format)`.

**Interactions**
- `Space` pause · `Esc` cancel (confirm) · `F` focus node · `/` search dots · arrow keys walk
  the tree mirror.
- Click node → inspector. Hover edge → relation + evidence. Drag node into the **focus well**
  → boosts its priority and the engine re-plans the frontier around it. Right-click → prune
  subtree.
- Export: PNG · SVG · `mindmap.json` (schema-versioned).

**States**
- *Empty:* the input, the tier row, and one line — "Type a topic, or pick a story from the
  ticker." **Nothing else.**
- *Loading (planning):* stage rail on `plan`, canvas shows the seed node only.
- *Error:* single line, specific, with the fix — "Ollama isn't responding on :11434 — start
  it, or switch routing to Cloud." Persists in the status bar.
- *Rejected (out of scope):* "Bhippi researches technology and AI only. 'sourdough
  hydration' scored 0.08." with the topic still editable.
- *Populated:* the live map.

**Acceptance:** 500 nodes at ≥ 55 fps · every claim reachable dot → source → URL in one
click · the map is fully navigable as a `role="tree"` list for screen readers · no physics in
JavaScript.

---

## A1b · Agent (default project screen — ADR-0006, amended by ADR-0010 and ADR-0012)

**Purpose:** converse with the engine; watch it think, read, and ask before it acts.

**Regions**
1. **App sidebar (persistent, §A0.4)** — owns navigation and the conversation list; the chat
   screen renders no rail of its own.
2. **Thread** — user/assistant turns; assistant turns stream in token-by-token with markdown.
3. **Activity strip inside an assistant turn** — engine tool-activity cards
   (`reading <source>` · `fetching` · `extracting dots`) each with spinner → result state,
   expandable detail. Collapses to one summary line when finished.
4. **Permission card** — when the engine needs a consequential action it renders an inline card:
   action, scope, risk, and **Allow once / Always / Deny**. The turn is blocked until answered.
5. **Composer** — a quiet context row above the input carrying project identity and any active
   branch or skills; the auto-growing input sends with Enter and inserts a newline with
   Shift+Enter. Beneath, one `+` menu consolidates attach/mention/command actions before the
   provider · model · effort controls. No persistent send button or keyboard-hint copy. While
   work is active, a tapered accent trace with a fading tail travels around the composer's
   rounded perimeter; reduced-motion mode replaces it with a static accent edge. `Esc` stops.
   Stop becomes Regenerate on a completed turn.

**States:** *no conversation open* — left-aligned greeting at 24 px with the glyph, a
`SESSIONS` eyebrow, recent conversations as hairline rows (dot · title · turn count ·
relative time · chevron), then the suggestion chips — never centred in an empty void, and no
scrollbar at 1240×820. *empty conversation* — "Nothing said here yet." + chips. *loading* —
hairline rail + pulsing phase label ("Thinking…", "Reading…"). *error* — single line with the
fix + Retry. *populated* — as above. Demo provider is always badged `demo`.

**Multi-session workspace (ADR-0023).** Multi mode renders every chat and CLI whose canonical
project path matches the active project. A compact toolbar shows the count and an **Organize**
popover: auto-fit plus Balanced columns, Adaptive tidy, and Smart fit. Panels use a 420 px control
floor and may be widened by pointer or keyboard only up to 72% of the available workspace; narrower
windows stack them. Each panel prints provider/type, title, and text status, can return to Single
mode, and keeps its stream, permission requests, composer, and terminal input isolated by turn id.
Loading, empty, error + Retry, and populated-at-volume states are explicit.

---

## A1c · Workbench — editor and browser (ADR-0014)

A right-hand pane holding the code editor, the local-preview browser, and (A1f / ADR-0020) the
game-engine workbench. **Closed on every launch**; its width (28–72 %, dragged on a hairline
splitter) and last mode are remembered. `Ctrl/Cmd+B` toggles it, `Ctrl/Cmd+'` cycles the modes,
`Ctrl/Cmd+3` jumps to Engine.

**The switch.** A three-label track with one sliding pill (Editor · Browser · Engine) — a single
translated element, not cross-fading backgrounds, which is what gives it weight. The curve
overshoots slightly and settles (`cubic-bezier(0.34, 1.42, 0.5, 1)`). `role="radiogroup"`, arrow
keys, and `aria-checked`; under `prefers-reduced-motion` the movement goes and the meaning stays.

**Editor.** A lazily-expanded file tree (Rust does the walking, skipping, and ordering), per-filetype
glyphs with desaturated tints, folder chevrons, and a code surface: line gutter, breadcrumb, tab
with a dirty dot, `Ctrl/Cmd+S`, Tab inserts two spaces. Highlighting is a per-line in-house
tokenizer returning spans, never an HTML string. Files over 1 MB and binaries open read-only and
say which of the two they are.

**Browser.** Loopback origins only — `localhost`, `127.0.0.1`, `0.0.0.0`, `::1`. Anything else is
refused with an explanation and offered to the system browser, where an address bar exists. The
port list shows only what a TCP probe reached during that call; idle ports are labelled with the
command that would start something on them, read from `package.json`. History is pane-local,
because a cross-origin frame will not report its navigation.

States: *empty* — "Nothing is running yet" with the probed port list and a re-check; *error* — the
exact filesystem or path-confinement failure; *populated* — tree plus file, or the framed preview.

## A1f · Engine — the game-engine workbench (ADR-0020, amended by ADR-0028)

A third mode of the same workbench surface. `Ctrl/Cmd+3` jumps straight to it; the Engine pane
stays mounted once opened, so switching modes never tears down the viewport or drops selection.
A **Maximize** control (`F11` inside the pane) expands the workbench over the chat column; the
chat stays reachable via a slim reopen handle (`Esc`/`F11` restores).

**Empty state** (no `Bhippi.game.toml` at the project root): "This project has no game manifest —
Create one?" with a one-click scaffold. **Error:** a fault card with Relaunch. **Loading:**
skeleton rows while the scene opens. **Populated:** the fixed layout below.

```
┌──────────────────────────────────────────────────────────────────────┐
│ Engine Toolbar (36px)  Play/Pause/Stop · Select|Move|Rotate|Scale ·   │
│ Grid/Snap ▾ · Camera ▾ · ⛶ Maximize · Scene: level_01 ● (Ctrl+S)      │
├───────────────┬──────────────────────────────────┬───────────────────┤
│ HIERARCHY     │                                  │ INSPECTOR         │
│ (220px, coll.)│         3D VIEWPORT              │ (280px, coll.)    │
│ ▸ level_01    │   Three.js webview renderer      │ name/tags/pos/…   │
│   ▸ Gameplay  │   exact-canvas AI observations   │ [+ Add component] │
├───────────────┴──────────────────────────────────┴───────────────────┤
│ [Assets] [Console] [Build]  (tabbed, 180px, collapsible)             │
└──────────────────────────────────────────────────────────────────────┘
```

- **Hierarchy** — entity tree (parent/child = transform hierarchy), drag-reparent, rename
  (`F2`), eye/lock per row, type-ahead filter. All writes are transactions; one undo step each.
- **Inspector** — renders the selected entity's components from the **reflection schema**
  exported by the viewport registry; zero hardcoded component layouts (render by field type:
  f32 drag-number, Vec3 triple, bool toggle, enum segmented, asset picker, colour swatch).
  Add Component searches the registry; ⋮ per component = remove/reset/copy as JSON (pasteable
  into chat). Every field edit is a transaction; drags coalesce to one undo step on release.
- **Content Drawer** — folder tree mirroring `assets/`; thumbnail grid (64/96/128px zoom) with
  type badges; drag into viewport = ray-cast instantiation (one transaction); double-click
  scene = open, script = opens in Editor mode (workbench modes cooperate); RMB → Import, New
  Folder/Scene/Script/Material, Rename, Delete (confirm), Reimport. A filesystem watcher keeps
  it live when the *AI* writes assets via `<write_file>`.
- **Console** — streamed structured log lines (level chips, target, collapse-repeats, filter,
  Clear, Copy). Script panics / asset faults render as FaultCards with remedy buttons incl.
  **"Ask agent to fix"**, which pre-fills the composer with the fault context.
- **Build** — target cards (Windows / Linux / macOS / Android / iOS / Web) with doctor
  readiness (✓ / ⚠ + Fix explainer), Debug/Release, live log stream, artifact row with
  Open folder / Run / (Web) Preview in Browser, build history from the DB.
- **Play** tint flips the toolbar amber. Pause/Resume, Step, Stop, Restart, 0.25×–2×,
  Game View and Eject/Possess stay in the transport bar with live fps/frame/entity/draw
  diagnostics. Runtime state is disposable; Stop returns to the byte-identical authored
  document (INV-081). Manifest gravity, `assets/input.json`, HUD bindings/actions and level
  travel are data-driven. Cuboid/sphere/capsule/heightfield colliders, slopes and steps run
  in the fixed-step kinematic solver. Rhai-subset source is compiled in Rust and executed by
  the bounded webview VM from ADR-0030; the pane never evaluates source text.
- **Keyboard (pane focus):** `Q/W/E/R` tools · `X` world/local · `F` frame · `Del` delete ·
  `Ctrl+D` duplicate · `Ctrl+Z/Y` undo/redo · `Ctrl+S` save scene · `Ctrl+Shift+P` commands ·
  `Ctrl+P` files/assets · `F11` maximize · `Ctrl+1..4` bottom tabs · `Ctrl+'` cycles mode.
- **AI surface:** the same world the human edits is visible to the agent via the Engine Mind
  Map (`.bhippi/engine/engine-map.json`), typed `<engine_action>` ops, and annotated
  exact-canvas screenshots and fixed-step scripted playtests — all through the same
  transaction/undo/journal machinery. Plans appear in the Activity Dock before application;
  Run Play is capability-gated and a timed-out/inactive pane is a typed failure, never a
  silent desktop screenshot.
- Chrome uses `tokens.css`; amber accent for selection; hairlines, never shadows; 120/200 ms
  motion; dark instrument aesthetic. All four states + keyboard reachability on every panel
  (INV-075). The viewport rectangle is framed with a 1 px hairline so it reads as part of the
  design system.

## A1d · Activity dock (ADR-0014)

Sits directly above the composer, inside its 780 px column.

Collapsed: an equaliser pulse, the current step's title breathing, and a summary — "3 steps
running", "thinking", "needs your answer". Opened: every step the engine reported for this turn,
each with its own action glyph, a state word, and elapsed time. Running rows breathe (opacity only,
2.6 s, so text never resamples); finished rows stop moving.

**The agent asks here.** A permission request opens the dock itself and renders the action, scope,
detail, risk chip, and Allow / Deny inline. The thing being watched and the thing being asked of
the user belong in one place; a question further up a scrolling thread arrives where nobody is
looking.

Every row corresponds to an emitted event (`chat-tool`, `chat-thinking`,
`chat-permission-requested`). Nothing is inferred. State is never carried by colour alone — the
state word is always present (INV-034).

## A1e · Project rules (ADR-0014)

A modal editing `.bhippi/rules.md` **inside the project folder**, reached from the toolbar. Rules
travel with the repository, can be committed and reviewed, and switching projects switches rules.
Rust reads the same file when assembling a turn (`prompts/chat-rules.md`), so what the panel shows
is what the agent is told. The prompt states that rules never widen access or override the
workspace boundary, the technology/AI scope, or any safety rule.

---

## A2 · Automation

**Purpose:** turn unattended operation on, and make it obvious what it will do.

**Regions**
1. **Mode switch** — Off / Timer / Ticker / Both.
2. **Plain-English summary**, updating live: *"Bhippi will research and publish up to 4 posts
   a day, between 07:00 and 23:30, with your review before publishing."*
3. **Next run** — countdown + the topic the picker currently intends, with its reason
   (uncovered cluster / coverage gap / refresh due / user queue).
4. **Queue** — drag to reorder; add a topic from anywhere in the app.
5. **Today's runs** — outcome per run: published / held for review / rejected out of scope /
   failed, each linking to its session.
6. **Review queue** — see A2.1.
7. **Dead-letter card** — appears only when a job failed 3 times; inspect payload and error,
   requeue or discard.
8. **Kill switch** — large, always visible on this screen.

**Data in:** `automation_status()` · events `automation.tick`, `budget.warning`.
**Data out:** `automation_set(config)` · `kill_switch()` · queue reorder commands.

### A2.1 Review queue (spec §16.4)

Each pending post shows: rendered preview · `fact_score` · `seo_score` · thin-evidence flags
· contradictions surfaced · image licence summary · for `refresh` runs, a **diff view**.
Actions: **publish** · **edit** (minimal markdown editor with live lint) · **send back for
deeper research** (re-runs one tier up, reusing the existing mind map) · **reject with a
reason** (the reason feeds style and interest memory).

**States:** *empty* — "Nothing waiting. Automation is off." / "…is on; next run in 42 min."
*error* — automation self-disabled after 3 consecutive failures, with the reason and a
"re-enable" button.

**Acceptance:** the summary sentence always matches the actual config · caps are visibly
enforced · kill switch stops everything within 3 s.

---

## A3 · Library

**Purpose:** everything published or drafted, dense and sortable.

**Regions**
1. **Table** — title · date · tier · words · fact score · SEO score · status · views (when a
   deploy target reports them). Dense rows, monospace numerics, sortable, filterable by
   status and tier, `/` to search.
2. **Row detail** — preview · metadata · **"open the mind map that produced this"** ·
   sources list with tiers · disclosure block · actions: refresh · retract · re-deploy ·
   open on site.
3. **Corrections** — a retracted post renders its correction notice and the original text
   struck through; the URL is never silently rewritten.

**Data in:** `review_queue()` · post list command · `post_preview(session_id)`.
**Data out:** `post_publish` · `post_refresh` · `post_retract(post_id, reason)`.

**States:** *empty* — "Nothing published yet. Run a research session or turn on automation."
*error* — last deploy failed, with the verify report inline.

---

## A4 · Settings (modal, 10 tabs, left rail)

Changes apply immediately and persist on blur. **No global Save button, no dialog that can be
lost.**

| Tab | Contents |
|---|---|
| **Appearance** | System / Light / Dark scheme cards with live previews; Bhippi's amber accent remains constant. The choice applies immediately and persists in the desktop webview. |
| **Providers** | Detected providers grouped CLI / API / Local. Row: vendor icon · model · context window · vision & tools badges · measured tok/s · health dot · enable toggle. `Re-scan` with live progress. `Add manually` (base URL + model + optional key → keychain). Routing selector (Quality / Balanced / Cheap / Local-only) with plain-language explanations. `Offline mode` master switch. Per-TaskClass override table, collapsed by default. |
| **Integrations** | Detected VS Code, Cursor, Antigravity, and platform file-manager launchers. Every row prints Available or Not detected with the exact PATH/install hint; active-project launch actions live in the project header. |
| **Usage** | Token and cost accounting per provider — see A4.0. |
| **Research** | Default tier with **the full budget table rendered** so the user sees what each tier buys. Anti-drift threshold. Counter-evidence toggle. Language. Search backend (SearXNG / Brave / Tavily / DDG) + **Test** button. Per-host rate limit. Concurrency. |
| **Ticker** | Feed table (name · tier · category · last fetch · health · enable). Add feed by URL with `<link rel=alternate>` auto-discovery. Poll interval. Burst threshold. Auto-trigger score **with a live histogram of the last 200 events** showing how many would have triggered — so the number means something. Category filters. |
| **Automation** | Mode · interval · daily cap · quiet hours · review gate · refresh-vs-new policy · budget caps · kill switch · the live plain-English summary sentence. |
| **Mind** | The global memory map — see A4.1. |
| **Skills** | The skill registry — see A4.2. |
| **Publishing** | Site name · URL · author identity · theme (Static / React) · deploy target + credentials (keychain) · build & preview · deploy history with rollback · SEO defaults · **disclosure text preview (non-removable)**. |

**Footer strip:** data directory path (click to open) · DB size · `Run doctor` · `Export
everything` · version + update check.

### A4.0 Settings › Usage (ADR-0009)

Window switcher (Today / 7 days / 30 days) · account refresh · stat tiles (tokens · estimated
cost · turns · who is answering) · the daily usage chart · a per-provider table containing the
detected account/workspace, plan, vendor-reported weekly percentage and reset, local guard,
tokens with the in/out split, turns, cost, and editable local daily cap. Providers remain in the
table even with zero Bhippi turns so account state is inspectable before first use. Unsupported
allowances say **Not reported**; they never become `0%`. `Clear history` arms on the first click
and acts on the second — never a browser `confirm()`.

Costs are **estimated list prices, never a bill**, and the screen says so in its own subtitle.
A provider with no price entry is not free, it is *not metered per token*; it shows a dash.

### A4.1 Settings › Mind (spec §11.5)

- **Constellation view** — entities as nodes, sized by mention count, coloured by kind,
  clustered by domain (AI research / chips / devtools / security / consumer / infra).
- **Session ribbon** — horizontal time axis of every session; hover shows its gist; click
  drops that session's nodes onto the constellation so the user sees what the run added.
- **Coverage heat** — many entities but few verified facts renders dim. These visible gaps
  are exactly what the Timer picker draws from.
- **Memory inspector** — searchable gist list with `use_count`, `decay_score`, last used;
  per item: pin (never decay) · edit · delete · **re-verify now**.
- **Controls** — decay half-life · max memory size · "forget topic X" · export
  (`memory.json`) · **Wipe memory** with typed confirmation.

Data: `memory_search(query)` · `memory_graph(filter)` · `memory_forget(target)`.

### A4.2 Settings › Skills (spec §17.4)

Two panes. **Left:** registry — name · version · kind · autonomy state · win-rate sparkline ·
last run. **Right:** detail — manifest · body (syntax highlighted, read-only unless editing)
· eval results table · run history · **capability requests rendered as an explicit permission
list**.

Actions: create (describe intent → engine drafts → user edits → eval → enable) · edit + bump
version · run against a test input · promote/demote autonomy · quarantine · delete ·
export/import as a `.bhippi-skill` folder.

**Pending approvals badge on the Settings gear. Nothing dangerous activates while the user is
not looking.**

---

# PART B — The generated blog (`themes/bhippi-default`)

Minimal, reading-first, dark-primary. Content column 68 ch. System font stack for body with
one self-hosted display face for headlines (subset, `font-display: swap`). A single accent
colour used **only** for links, the ticker's live dot, and the reading-progress rail. No
gradients, no shadows, no card decoration.

**Budgets [HARD REQ]:** ≤ 40 KB CSS · ≤ 25 KB JS on an article route · zero third-party
scripts · zero cookies · no analytics by default · article page ≤ 120 KB total · LCP ≤ 1.8 s
on 4G · Lighthouse SEO ≥ 95, Performance ≥ 90, A11y ≥ 95.

| # | Page | Route | Contents |
|---|---|---|---|
| B1 | Home / latest | `/` | Header (wordmark · search · theme toggle) · optional live ticker strip · lead post · dense list of recent posts with dek, date, reading time, tier badge · pagination `rel=prev/next` |
| B2 | Article | `/<slug>/` | Reading-progress rail · H1 · dek · hook · "What happened" · hero image · H2 sections · pull-out boxes ("What's disputed", "What we still don't know") · "Why it matters" · **numbered sources with publish dates and tier badges** · methodology footer (tier, sources examined, session id, models used) · **AI disclosure** · mind-map embed toggle · related posts · sticky ToC ≥ 1100 px |
| B3 | Archive | `/archive/` | All posts, grouped by month, filterable by tag and tier |
| B4 | Tag / category | `/tag/<slug>/` | **Real intro copy, never a bare list** · posts in that tag |
| B5 | About + Methodology | `/methodology/` | **[HARD REQ]** how posts are produced: the depth ladder, source tiers, the fact gate, image licensing, correction policy, and what "reviewed by a human" means on this site |
| B6 | Search | `/search/` | Hydrated island over a prebuilt index; works without JS by falling back to the archive |
| B7 | Ticker archive | `/live/` | Past ticker clusters with what Bhippi covered and what it skipped |
| B8 | Mind map viewer | `/<slug>/map/` | The session's `mindmap.json` rendered read-only, lazily hydrated |
| B9 | 404 | `/404.html` | One line, a search box, and the archive link |
| B10 | Feeds & robots | `/rss.xml`, `/feed.json`, `/sitemap.xml`, `/robots.txt` | Generated, accurate `lastmod` |

**Article page invariants**
- Every claim traces to a numbered source; every source shows date and tier.
- Attribution renders under any image whose licence requires it.
- The AI disclosure is visible **and** machine-readable, and cannot be switched off in the UI.
- A retracted post shows a correction notice with the original struck through, at the same URL.
- Images: hero preloaded and LCP-optimised, everything below the fold lazy; one image per
  400–600 words; never two adjacent without text between; alt text functional, ≤ 125 chars.

**Interactive islands (loaded on demand only):** search, mind-map viewer, ticker strip.
Nothing else hydrates.

---

## Cross-cutting: empty, loading, error (spec §19.4)

| State | Rule |
|---|---|
| Empty | One sentence that says what to do next. Never an illustration, never a tour. |
| Loading | A 1 px hairline progress rail. No spinners, no skeleton shimmer. |
| Error | Single line, specific, carries the fix. Persists in the status bar until dismissed. |
| Blocked | When a gate holds a post, say **which gate and why**, with a link to the evidence. |

## Cross-cutting: accessibility floor [HARD REQ]

Keyboard reachable everywhere · visible focus rings in `--accent` · AA contrast minimum
(verify `--text-dim` and `--text-faint` against `--surface`) · ticker pausable and
reduced-motion aware · mind map mirrored as a `role="tree"` list · no colour-only meaning
(contradiction edges are dashed as well as red) · every interactive control has an accessible
name.
