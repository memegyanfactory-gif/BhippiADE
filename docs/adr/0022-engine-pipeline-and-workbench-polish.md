# ADR-0022: Workbench expansion, native browser, Grok weekly probe, Unreal pipeline slice

- **Status:** Accepted
- **Date:** 2026-08-30
- **Derives from:** ADR-0014, ADR-0020, ADR-0021, `prompt.md`, owner request 2026-08-30
- **Supersedes:** ADR-0014 loopback-only in-panel browser (external sites now use a native child webview, not an iframe)
- **Amends:** ADR-0020 §v1 exclusions — **file-based** shaders and UltraSky-style weather templates are in scope. A **visual shader graph** remains excluded until its own ADR.

## Decision

1. The workbench splitter may grow past 50% of the split, but is **hard-locked** so chat never shrinks below 540px and the workbench never takes more than 68% of the split. That stop is the owner screenshot: composer controls stay on one row and must not overlap. CSS enforces the same floor (`min-width: 540px` on the chat column, `max-width: calc(100% - 540px)` on the workbench).
2. The Browser pill hosts a Tauri child `Webview` labelled `workbench-browser` so ordinary websites load. Iframe is only the non-Tauri fallback. Pop-out windows keep `browser-*` / `pip-*` labels, which must be in the capability.
3. Grok weekly usage is probed from `grok -p /usage` (max-turns 0) and `grok dashboard`. Missing data stays **Not reported**.
4. Engine empty state: no `Bhippi.game.toml` → empty grid, empty Content Drawer, same chrome. Opening Engine never auto-scaffolds. New Game writes Main + HUD + level_01.
5. File-based materials (`*.mat.json`) and shaders (`*.shader.json`) are assignable. Weather presets live under `assets/weather/`.

## Consequences

IPC may grow `import_workspace_file`, richer `EngineStatus`, and mind-map sync. Bindings must be regenerated. Three.js remains the viewport stand-in until ENG-010.
