# ADR-0029: Plugin gallery, a catalogue in Rust, and per-plugin icon tints

Date: 2026-09-01 · Status: accepted · Supersedes: (none — first Plugins screen)

## Context

The Plugins screen existed as a stub. The main pane read "Browse, activate and install
plugins from the panel in the sidebar", and the sidebar panel it pointed at read
`~/.bhippi/plugins/*.json` directly. On a fresh machine that directory does not exist, so
the whole feature rendered as an empty box — a screen that tells the user to go somewhere
else, where there is nothing.

Three specific problems sat behind that:

1. **No catalogue.** Bhippi already ships browser, terminal, git, research, automation,
   memory, assets and usage capability. Nothing told the user those exist or let them be
   switched off. "Installed" was only ever true for something hand-written into the
   plugins directory.
2. **The plugins directory defaulted to `.bhippi/plugins`, relative to the working
   directory.** A user's installs landed wherever the app was launched from, so they did
   not survive a relaunch from a different shell.
3. **The old `PluginMetadata` was both the wire format and the storage format.** Adding a
   field to the screen would have made every previously written file fail to parse.

## Decision

### The catalogue lives in Rust, and so does every derived fact

`crates/bhippi-app/src/plugins.rs` owns a `CATALOG` of ten entries, the on-disk
`PluginRecord`, and the merge between them. The screen receives `PluginMetadata` with the
badge (`status`) and the one primary button (`action`) **already decided**. The React
screen filters, searches and sorts; it never works out what a plugin's state is
(INV-032 — no business logic in TypeScript).

Storage and wire format are now separate types. `PluginRecord` is `#[serde(default)]` on
every field, so a partially written or older file loads instead of blanking the screen,
and one corrupt file is skipped with a warning rather than failing the listing.

Plugin ids become file names, so `safe_id` reduces any id to `[a-z0-9-]` and rejects what
is left if it is empty or over 64 characters. An id can never reach outside `plugins_dir`.

### `plugins_dir` defaults beside `config.toml`

`~/.bhippi/plugins`, not `./.bhippi/plugins`. `workspace.plugins_dir` still overrides it.

### Status is earned, never decorated

The five badge states are all implemented, but each one has to be true:

| Badge | When |
|---|---|
| `Built-in` | ships inside the binary (terminal, memory) — switchable, never removable |
| `Installed` | a capability we ship, or a record the user installed |
| `Needs Setup` | the entry declares a required Settings tab and is not installed yet (deployment) |
| `Beta` | the entry is genuinely unfinished (website — publishing is S8, still `todo`) |
| `Update Available` | an installed **record's** version is behind the catalogue's |

`Update Available` is therefore absent on a fresh machine, and appears only once a record
really has fallen behind. A badge is never rendered from a constant that says so.

Uninstalling a pre-installed entry writes `installed: false` rather than deleting the
file, so the next listing does not quietly re-install it. Uninstalling a built-in is
refused with a hint pointing at the toggle.

### Per-plugin icon tints — a deviation from the one-accent rule

`docs/04-PAGES.md` ("Chrome uses `tokens.css`; amber accent for selection") and
`tokens.css` ("One accent (amber → orange)"; the chart palette is "CHART MARKS ONLY —
never chrome") would put every one of the ten icon tiles in the same amber.

We deviate **only** for the plugin card's 42 px icon tile, which carries a hue from a
`TINTS` map in `ui/src/screens/Plugins.tsx`, applied through a `--plugin-tint` custom
property. Rationale: ten cards in a five-column grid are scanned by shape and colour
before they are read, and an all-amber grid reads as ten copies of the same thing.

The deviation is bounded:

- Tints apply to the icon tile only. Buttons, badges, tabs, focus rings and the toggle
  stay on `--accent` and the neutral ramp.
- Colour is never the sole carrier of meaning: every card states its status in words and
  its action in words (INV-034).
- The tint is one CSS custom property per card, so reverting to `var(--accent)` is a
  one-line change in `TINTS`.

## Consequences

- `list_plugins` returns a full catalogue on a machine with no plugins directory, so the
  screen has content on first run.
- Two new commands (`uninstall_plugin`, `update_plugin`) join the four existing ones;
  `activate_plugin` / `deactivate_plugin` now write a record for a catalogue entry that
  has none, so switching a built-in off persists.
- `activate_plugin` on something not installed is now an error with a hint, so the sidebar
  panel lists only installed plugins; the full catalogue is the screen's job.
- `ui/src/lib/ipc.ts` regenerated (INV-032).
- A future real plugin runtime (sandboxing, permissions, actually loading code) is **not**
  decided here. Today a catalogue entry routes to a capability the app already has, and a
  URL install records a window to open. That boundary is deliberate: nothing in this ADR
  executes third-party code.
