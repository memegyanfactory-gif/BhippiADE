# 09 · UI revision brief — chat shell, model selector, effort slider, CLI fix

Status: **open** · Written 2026-08-26 · Owner: whoever picks it up next
Authority: this brief sits **below** `00-SPEC`, `01-ARCHITECTURE`, `06-INVARIANTS` and the
ADRs. Where it disagrees with them, they win and this file is wrong — say so rather than
deviating silently.

This is a complete, self-contained work order. An agent with no other context should be able
to execute it from this file plus the repository. Every "current state" claim below was
verified in the working tree on 2026-08-26; do not take them on trust if the tree has moved —
re-check, then correct this file.

---

## 0 · What is being asked, in one paragraph

Four things, in this order of importance:

1. **W1 — Fix the provider failure.** Chat through Claude Code dies with
   `provider 01M0YNMSD7PXZV0GT67KVKZTTD unavailable: the CLI answered with nothing`. Two
   separate bugs are stacked here: a wrong identifier in the message, and a real spawn
   failure. Root causes are diagnosed below; this is not a hunt.
2. **W2 — Add a model selector** beside the provider picker, listing **only the models of the
   provider currently selected**, and actually send that model to the backend. Today
   `CompletionRequest` has no `model` field at all, so this is a real end-to-end change, not
   a dropdown.
3. **W3 — Rebuild the effort control** as a squarish slider with a particle field behind the
   track and a squarish knob, matching the reference image.
4. **W4 — Rework the chat shell** to match the reference layout: navigation moves into the
   left sidebar, the top chrome above it goes away.

W1 blocks the product being usable at all. Do it first and independently — it touches no file
that W2–W4 touch except `cli.rs`, which W2 also edits.

---

## 1 · Orientation: the repository as it stands

Bhippi is a Rust + Tauri v2 desktop app. Rust workspace at the root, React 18 + TypeScript +
Vite frontend in `ui/`.

```
crates/
  bhippi-types/       ids, errors, budgets           (L1, no deps on siblings)
  bhippi-core/        config, event bus, logging, usage ledger
  bhippi-providers/   catalogue, detection, adapters, the Provider trait
  bhippi-app/         Tauri shell: IPC commands, chat engine, usage summary
  … 10 more crates, not touched by this brief
ui/src/
  App.tsx             shell composition, status polling
  chrome/             TitleBar.tsx · TickerStrip.tsx · StatusBar.tsx · UsageMeter.tsx
  screens/            Chat.tsx (790 lines) · Research · Automation · Library
                      SettingsModal.tsx · UsagePanel.tsx
  components/         icons.tsx · ProviderLogo.tsx · Markdown.tsx · UsageRing.tsx
  lib/                ipc.ts (GENERATED) · api.ts · format.ts
  styles/             tokens.css · app.css · chat.css · screens.css · usage.css
```

### Rules you cannot break

| Rule | What it means here |
|---|---|
| **INV-032** | `ui/src/lib/ipc.ts` is **generated**. Never hand-edit it. After changing any `#[tauri::command]` or `Type`-deriving struct, run `cargo run -p bhippi-app --bin export-bindings`. |
| **INV-036** | No `unwrap()` / `expect()` anywhere, **including tests** — the workspace lints deny both. Tests use `unwrap_or_else(\|error\| panic!("…: {error}"))` for `Result` and `unwrap_or_else(\|\| panic!("…"))` for `Option`. Follow the existing style. |
| **INV-003** | Provider CLIs spawn with **explicit argv**, a scrubbed environment and a timeout — never a shell string. W1 changes what is in the scrubbed environment; it must not change *that* it is scrubbed. |
| **INV-034** | Keyboard reachable, visible focus, AA contrast, `prefers-reduced-motion` honoured, **no colour-only meaning**. The effort slider in W3 must therefore also name its level in text. |
| **No business logic in TypeScript** | TS formats and lays out. It must not compute a cost, pick a model on the user's behalf, or invent a fallback figure. |
| **ADR first** | A structural deviation needs an ADR in `docs/adr/` **before** the code, and the affected doc amended in the same change. |

### Gates that must be green before you call anything done

```bash
cargo +stable fmt --all --check
cargo +stable clippy --workspace --all-targets -- -D warnings
cargo +stable test --workspace
cargo +stable run -p bhippi-app --bin export-bindings   # then confirm ipc.ts has no stray diff
cd ui && npx tsc --noEmit && npx vite build
```

### Seeing your change in the real app

`ui/dist` is **embedded into the executable**, so a rebuilt frontend alone changes nothing:

```bash
cd ui && npx vite build
cd .. && cargo clean -p bhippi-app          # mandatory, or the exe ships stale assets
cargo +stable build -p bhippi-app --bin bhippi-desktop
./target/debug/bhippi-desktop.exe
```

Kill any running instance first (`Get-Process bhippi-desktop | Stop-Process -Force`) or
`cargo clean` fails with "Access is denied".

---

## W1 · Fix the provider failure  **[do this first]**

### The reported symptom

```
provider 01M0YNMSD7PXZV0GT67KVKZTTD unavailable: the CLI answered with nothing —
it may need a login or different flags
**Fix:** Check that Claude Code runs — reinstall it from Settings › Providers.
```

There are **two independent defects** here. Fix both.

### W1a — The identifier is a random ULID, not the provider

`crates/bhippi-providers/src/cli.rs`, in `CliProvider::error`:

```rust
BhippiError::Provider {
    id: self.id().parse().unwrap_or_else(|_| ProviderId::new()),
    …
}
```

`ProviderId` is a **ULID newtype** (`crates/bhippi-types/src/ids.rs`, the `define_id!` macro).
`"claude".parse::<ProviderId>()` can never succeed, so every CLI error mints a fresh random
ULID and prints it at the user. The same pattern is in `ollama.rs` (2 sites),
`openai_compat.rs` (2 sites) and `provider.rs` (1 site) — grep `BhippiError::Provider` and fix
all of them together.

**The fix.** Catalogue ids are stable strings (`claude`, `codex`, `ollama`, …); they are not
and never will be ULIDs. Change the error variant to carry a human-readable name:

```rust
// crates/bhippi-types/src/error.rs
#[error("provider {id} unavailable: {reason}")]
Provider {
    /// The catalogue id or vendor label — what the user would recognise, never a ULID.
    id: String,
    reason: String,
    retryable: bool,
    hint: Option<String>,
},
```

Then every construction site passes `self.spec.label.to_owned()` (or `self.id().to_owned()`
where no label is in scope). Prefer the **label** — the user knows "Claude Code", not
`claude`.

Check whether `ProviderId` is still constructed anywhere after this. If nothing uses it,
leave the type in place (the `providers` DB table and future routing will want it) but do not
reintroduce it into user-facing error text.

> If you would rather not widen `BhippiError`, the alternative is a `provider: String` field
> added alongside `id`. That is worse — two identifiers, one of which is always noise. Take
> the change above unless an invariant forbids it, and write an ADR if you deviate.

### W1b — The CLI really does return nothing, and the cause is `PATHEXT`

**This was reproduced on the developer machine on 2026-08-26. It is not a theory.**

`crates/bhippi-providers/src/command.rs` scrubs the child environment down to `SAFE_ENV_KEYS`
(INV-003, correct in principle). That list is missing **`PATHEXT`**.

On Windows, `claude` resolves to `%APPDATA%\npm\claude.ps1` — an npm PowerShell shim whose
last line is effectively `& "node$exe" "…/cli.js" $args`. Without `PATHEXT`, PowerShell cannot
resolve `node` to `node.exe`, the shim's invocation fails silently, and the child **exits 0
with empty stdout**. `cli.rs` then takes its empty-output branch and reports "the CLI answered
with nothing".

Reproduction, both directions:

```
inherited environment                          → EXIT=0, stdout "pong"
scrubbed to SAFE_ENV_KEYS                      → EXIT=0, stdout ""        ← the bug
scrubbed to SAFE_ENV_KEYS + PATHEXT            → EXIT=0, stdout "pong"    ← the fix
```

**The fix.** Add `PATHEXT` to `SAFE_ENV_KEYS`. While you are there, add the other Windows
variables a Node process legitimately expects and that leak nothing sensitive:

```
"COMSPEC", "NUMBER_OF_PROCESSORS", "OS", "PATHEXT",
"PROCESSOR_ARCHITECTURE", "PSMODULEPATH", "USERNAME",
```

Do **not** widen this to `env::vars()` — the scrub is an invariant, not a nuisance. Each key
added is a deliberate decision; keep the list alphabetical and leave a comment saying why
`PATHEXT` is load-bearing so nobody "tidies" it away.

**Regression test** (put it in `crates/bhippi-providers/src/command.rs` tests, next to
`resolves_and_launches_a_windows_npm_powershell_shim`):

Write a temporary `.ps1` shim that prints `$env:PATHEXT`, spawn it through
`ResolvedCommand::command()`, and assert the output is non-empty. That pins the regression
without depending on a real vendor CLI or the network.

### W1c — `.cmd` is not in the launcher candidate list

`candidate_names()` in `command.rs` tries `.exe`, `.com`, `.ps1` on Windows. npm installs
**`claude`, `claude.cmd`, `claude.ps1`** — the `.cmd` is the more robust launcher and is
skipped entirely.

Add `.cmd` to the candidate list, ordered `[".exe", ".com", ".cmd", ".ps1"]`, and teach
`resolved_from_path` to run a `.cmd` through `cmd.exe /c` the way `.ps1` goes through
PowerShell. Extend the existing candidate-names test.

This is a robustness improvement, not the root cause. Do it after W1b is proven.

### W1d — Make the empty-output failure legible

Even fixed, an empty answer will happen again (not logged in, quota exhausted). The message
should say what to try, in this order: (1) run the CLI once in a terminal to confirm it is
signed in, (2) check the account is not rate-limited, (3) reinstall from Settings › Providers.
Keep it to one `hint:` line — the existing `R1` actionable-error contract.

### W1 acceptance

- [ ] Chat through Claude Code returns an answer in the desktop app.
- [ ] Every provider error names the provider (`Claude Code`), never a ULID. Grep for
      `ProviderId::new()` in `bhippi-providers` and confirm zero hits.
- [ ] A test fails if `PATHEXT` is dropped from the child environment again.
- [ ] `.cmd` launchers resolve and run.

---

## W2 · Model selector

### Current state — read this before designing anything

- `ProviderInfo` (generated into `ipc.ts`) **already carries** `models: string[]`, populated by
  `bhippi-providers::detect` / `extract_model_names`.
- `Chat.tsx` has a `ModelPicker` component, but it picks a **provider**, not a model. It shows
  `option.version ?? option.models[0]` as a subtitle and throws the rest away.
- **`CompletionRequest` has no `model` field.** `crates/bhippi-providers/src/model.rs` defines
  `task`, `system`, `messages`, `max_tokens`, `temperature`, `json_schema`, `timeout`. Nothing
  more.
- `send_chat_message` / `regenerate_last_answer` take `(conversation_id, text, provider_id,
  effort)`. No model parameter.

So the dropdown is the last 10 % of this task. The plumbing is the work.

### Backend changes

1. **`CompletionRequest` gains `pub model: Option<String>`** — `None` means "the vendor's own
   default", which is the honest representation of a CLI that was given no `--model` flag.
   `CompletionRequest::new` sets it to `None`.

2. **Each adapter honours it:**
   - `cli.rs` — the catalogue's `prompt_args` template gains an optional model segment. Add
     `model_args: Option<&'static [&'static str]>` to `ProviderSpec`, e.g.
     `Some(&["--model", "{model}"])` for Claude Code and Codex. When `req.model` is `Some`,
     substitute and append; when `None`, append nothing. Never pass an empty `--model`.
   - `ollama.rs` — `/api/chat` takes `"model"` in its JSON body. It almost certainly already
     hardcodes or defaults one; make `req.model` win when present.
   - `openai_compat.rs` — same, the `"model"` field of the request body.
   - Unknown/unsupported model on a given adapter must surface the vendor's own error, not be
     silently swapped. **No silent fallback** — that is the ADR-0006 spirit and INV-001's
     nearest cousin.

3. **IPC:** `send_chat_message` and `regenerate_last_answer` gain `model: Option<String>`,
   threaded through `ChatEngine::send` / `regenerate` / `start_assistant` / `run_turn` into the
   request. Regenerate the bindings.

4. **Persistence:** remember the last model per provider so switching provider and back does
   not lose the choice. Put it in `config.toml` under `providers` as
   `last_model: BTreeMap<String, String>` (provider id → model). `BhippiConfig` uses
   `#[serde(default, deny_unknown_fields)]`, so adding a field is backward compatible.

5. **Attribution:** the usage ledger keys on provider id today. Leave it that way for now —
   per-model accounting is a separate ticket. Do not silently change the ledger's key shape;
   `ProviderTally` is persisted on disk at `~/.bhippi/usage.json`.

### Frontend changes

Two adjacent controls in the composer, in this order: **provider**, then **model**.

- Rename the existing `ModelPicker` to `ProviderPicker` (it always was one). Keep its drop-up
  behaviour and styling.
- Add a `ModelPicker` that is genuinely a model picker:
  - Its options are **exactly `selectedProvider.models`** — never the union, never another
    provider's list.
  - Selecting a different provider **resets** the model to that provider's remembered choice,
    or its first model, or "Default" when the list is empty.
  - When the provider reports **zero** models, render the control as a disabled chip reading
    `Default model` with a `title` explaining that this backend does not advertise a model
    list. Do not hide the control — a control that appears and disappears is worse than a
    quiet disabled one.
  - Long model ids (`claude-sonnet-4-5-20250929`) must not blow the composer width: truncate
    with a middle ellipsis in the button, show the full string in the drop-up and in `title`.
- Both pickers close on Escape and on click-away, and neither steals focus from the textarea.
- The drop-up already exists as `.dropup` / `.dropup-item` / `.dropup-name` / `.dropup-model`
  in `chat.css`. Reuse it. Do not invent a second popover style.

### W2 acceptance

- [ ] Selecting Claude Code shows only Claude models; selecting Ollama shows only local ones.
- [ ] The chosen model reaches the vendor — provable by an adapter unit test asserting the
      built argv/body contains it, and by an answer that differs when the model differs.
- [ ] `model: None` sends no `--model` flag at all.
- [ ] The choice survives an app restart.
- [ ] A provider with an empty model list is usable, with the control disabled and explained.

---

## W3 · The effort slider — squarish track, particle field

### Reference

The second attached image: a small popover reading `Effort  Ultracode` (label dim, value in
the accent), `Faster` on the left and `Smarter` on the right, a **squarish rounded-rectangle
track** filled with a fine dotted particle texture, and a large **rounded-square knob** sitting
at the right end.

### Current state

`Chat.tsx` → `SpeedPicker` renders `.speed-track` with `.speed-dots`, four round
`.speed-dot`s and a round `.speed-knob` positioned by `left: %`. `chat.css` around line 576
holds `.speed-menu` and friends. The four levels are `fast | balanced | quality | ultra`
(`EFFORT_LEVELS` in `Chat.tsx`; the Rust side is `Effort` in `crates/bhippi-app/src/chat.rs`,
which maps each level to real `max_tokens`, `temperature` and a system directive — **do not
change that ladder**, only its presentation).

### Target

**Geometry.** Track: full popover width, `height: 28px`, `border-radius: 8px` — squarish, not
a pill. Knob: `24×24`, `border-radius: 6px`, inset 2px from the track edge, travelling
`left: calc(index / 3 * (100% - 24px))`. Knob fill `--text` (near-white), no shadow — a 1px
`--line-strong` hairline instead, per the house rule.

**The particle field.** A CSS-only dot lattice behind the knob, inside the track:

```css
.speed-track-rail {
  background-color: var(--surface-2);
  background-image: radial-gradient(circle at center, var(--text-faint) 1px, transparent 1px);
  background-size: 6px 6px;
  background-position: 0 0;
}
```

The portion **to the left of the knob** (the "already spent" side) is a separate absolutely
positioned layer whose dots are tinted `--accent` at reduced opacity and whose `width` matches
the knob position, so the field reads as filling up as the user drags toward Smarter. Two
layers, one mask; no canvas, no library, no per-particle DOM node.

**Motion.** Knob and fill transition on `--dur-fast` with `--ease`. Add a *single* subtle
drift on the particle layer — `background-position` animating 6px over ~8s, `linear`,
`infinite` — so the field feels alive rather than printed. It must be inside a
`@media (prefers-reduced-motion: no-preference)` block; the global reduced-motion rule in
`tokens.css` already collapses durations, but an infinite ambient animation should not merely
be shortened, it should not run at all.

**Labels.** `Faster` and `Smarter` at `--fs-micro` in `--text-dim`, outside the track. The
current level's **name** is printed in the header (`Effort` dim, the level name in `--accent`)
— that is what satisfies INV-034, since knob position alone is a spatial-only signal.

**Interaction — all three must work:**
- Click anywhere on the track → snap to the nearest of the four stops.
- Drag the knob → snap on release.
- Focus the track and use `←` / `→` / `Home` / `End`. The track is
  `role="slider"` with `aria-valuemin=0 aria-valuemax=3 aria-valuenow={index}` and
  `aria-valuetext="Balanced"`. This is not optional.

Keep the existing four-item list under the slider — it names each level and its blurb, and it
is the accessible fallback.

### W3 acceptance

- [ ] Track and knob are squarish, matching the reference proportions.
- [ ] A particle lattice is visible behind the knob and tints on the spent side.
- [ ] Keyboard, click and drag all move it; the level name is always readable as text.
- [ ] With `prefers-reduced-motion: reduce`, nothing drifts.
- [ ] The `Effort` → `max_tokens` / `temperature` / directive mapping is byte-identical to
      before.

---

## W4 · Chat shell to match the reference

### Reference

The first attached image. Read it as a **layout brief**, not a skin to clone: Bhippi is its own
product and must not ship another vendor's wordmark, mascot or copy.

### What changes

**Remove the top chrome.** Today `App.tsx` renders a four-row grid:
`36px ticker · 44px title bar · 1fr screen · 28px status bar`. The reference has no top tab
bar; navigation lives in the sidebar. Target:

- Drop `TickerStrip` from the chat shell. It is a real feature (spec §15.3) — do not delete
  the component. Move it into the Research screen, or behind a toggle, and record the move in
  `docs/04-PAGES.md §A0.1`.
- Collapse the title bar to a slim draggable strip carrying only the window controls, the
  wordmark and the settings gear. The four screen tabs (`Chat / Research / Automation /
  Library`) move into the sidebar as icon+label nav rows.
- Keep the status bar. The usage gauge lives there and it is 28px — it costs nothing.

**The sidebar** (~280px, `--surface`, hairline right border):
- A compact icon row at the top: panel-collapse, search, back, forward.
- A full-width `+ New` button, quiet — hairline border, `--surface-2` on hover, not a filled
  accent block.
- Nav rows for the four screens, icon + label, the active one marked with an
  `inset 2px 0 0 var(--accent)` rail (the idiom already exists in `chat.css:85`).
- A section header for conversations: small, uppercase, `--fs-micro`, `--text-dim`, with a `+`
  affordance on the right.
- The conversation list, each row a status dot + title, truncated to one line.
- Bottom: the account/identity row, hairline-separated.

**The main area** when no conversation is open:
- Top-right: one quiet link slot (currently unused — leave it out rather than inventing a
  "What's new" that goes nowhere).
- A left-aligned greeting at `--fs-xl`, weight 500, with the Bhippi glyph before it. **Not
  centred** — the reference aligns to the content column, and centred text at this size reads
  as a marketing page.
- A `Sessions` / `Recent` label at `--fs-micro` uppercase, then recent conversations as
  **rows**, each: status dot · title in `--text` · one-line preview in `--text-dim` · relative
  time · a right chevron. Hairline separated, no card fills, no shadows.
- The existing four suggestion chips are good and should survive — put them below the recent
  rows, not centred in an empty void.

**The composer:**
- A context-chip row above the input (the reference shows Local / repo / branch / worktree).
  Bhippi's equivalents are the *scope* facts it actually has — do not invent chips for
  concepts the app does not model. If only one is real today, ship one.
- The textarea, with the send affordance right-aligned inside it.
- A control row beneath: provider picker · model picker (W2) · effort control (W3) on the
  left; the send/newline hints on the right.

### Constraints

- Every colour from `tokens.css`. Every size and gap from the type and spacing scales.
- Hairlines, never shadows. No gradients (the particle lattice in W3 is a repeating
  `radial-gradient` used as a texture, which is the one sanctioned exception — it is not a
  colour ramp).
- All four states per list: loading, empty, error, populated at real volume.
- `docs/04-PAGES.md §A0` must be amended in the same change to describe the new chrome. If the
  ticker moves screens, that is a structural change to a documented page — write the ADR.

### W4 acceptance

- [ ] No top tab bar; the four screens are reachable from the sidebar by mouse and keyboard.
- [ ] The empty chat state shows greeting, recent rows and suggestions without a scrollbar at
      1240×820 (the app's default window size, `crates/bhippi-app/tauri.conf.json`).
- [ ] The ticker still exists somewhere and its doc entry matches reality.
- [ ] No Bhippi-external branding, mascot or copy anywhere in the diff.

---

## 2 · Suggested order and why

| # | Work | Touches | Independent of |
|---|---|---|---|
| 1 | W1a + W1b | `bhippi-types/error.rs`, `bhippi-providers/{cli,ollama,openai_compat,provider,command}.rs` | everything else |
| 2 | W1c + W1d | `command.rs`, `cli.rs` | everything else |
| 3 | W2 backend | `model.rs`, `catalog.rs`, three adapters, `chat.rs`, `commands.rs`, `lib.rs`, `config.rs` | W3, W4 |
| 4 | W2 frontend | `Chat.tsx`, `chat.css`, `api.ts` | W3 |
| 5 | W3 | `Chat.tsx` (`SpeedPicker`), `chat.css` | W4 |
| 6 | W4 | `App.tsx`, `chrome/*`, `Chat.tsx`, `app.css`, `chat.css`, `docs/04-PAGES.md` | — |

W3 and W4 both edit `Chat.tsx` and `chat.css`. If two agents run in parallel, give W3 the
`SpeedPicker` function and the `.speed-*` CSS block and nothing else, and give W4 everything
above the composer. Otherwise do them in sequence.

---

## 3 · Things that will bite you

- **`cargo clean -p bhippi-app` after every `vite build`**, or the desktop binary serves the
  previous frontend and you will debug a ghost.
- **The lints deny `expect()` in tests.** Copy the `unwrap_or_else(|error| panic!(…))` idiom
  from `crates/bhippi-db/tests/database.rs`.
- **`ipc.ts` is generated.** If `tsc` complains about a type you "know" exists, you forgot to
  re-export the bindings.
- **`specta` versions are pinned as a set** — `tauri-specta =2.0.0-rc.24`, `specta
  =2.0.0-rc.24`, `specta-typescript 0.0.11`. Do not bump one; `specta-typescript` 0.29.x no
  longer resolves from the index (ADR-0004, ADR-0007).
- **The architecture guard is strict.** `cargo test -p bhippi-types --test architecture` fails
  on any new crate edge. Adding one requires an ADR (see ADR-0008 for the precedent).
- **The usage ledger is live on disk** at `~/.bhippi/usage.json`. If you seed fake data to
  look at a UI state, back the file up and restore it — do not leave invented numbers in the
  owner's real ledger.

---

## 4 · Definition of done for the whole brief

1. All five gates in §1 pass.
2. The desktop app has been **launched and looked at**, not just built — a real answer
   streamed from a real provider, the model selector switched, the effort slider dragged.
3. `docs/04-PAGES.md` matches the shipped chrome.
4. `docs/PROGRESS.md` gains a ticket row and a session-log line: what you did, what you
   learned that this file did not say, and what the next agent should do first.
5. Anything you could not finish is named explicitly, with the reason — not quietly dropped.
