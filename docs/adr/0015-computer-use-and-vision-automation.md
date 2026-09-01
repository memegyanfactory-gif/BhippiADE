# ADR-0015 — Computer Use and Full PC Vision Automation

- **Status:** Accepted
- **Date:** 2026-08-26
- **Supersedes:** nothing
- **Relates to:** ADR-0006 (chat surface), ADR-0008 (provider edges), ADR-0013 (project workspaces), ADR-0014 (workbench & activity dock)

## Context

Users need the AI agent to operate beyond text generation and workspace filesystem edits by directly interacting with the desktop computer: observing the user interface via high-resolution screen vision and executing autonomous input actions (mouse movements, clicks, scrolling, text typing, shortcuts, and window actions).

This capability introduces three critical requirements:
1. **Vision capability requirement**: Desktop UI navigation requires spatial reasoning over pixel coordinates. Text-only models cannot parse UI visual layouts.
2. **Provider authorization policy**: Only **Claude Code** (`claude`), **Codex CLI** (`codex`), and **Grok CLI** (`grok`) possess multimodal vision reasoning capable of computer use. **OpenCode** (`opencode`) and local text-only backends do not support vision and must be explicitly blocked from computer use.
3. **Full PC control & settings visibility**: The user must have a dedicated control center in the Settings panel with an explicit master toggle, full-access permission switch, provider compatibility matrix, and live screen preview testing.

## Decision

### 1. Settings Panel Centerpiece
Settings acquires a dedicated **Computer Use** section with:
- A master toggle (`computer_use.enabled`).
- A full PC access permission switch (`computer_use.full_access`).
- An authoritative provider support list prominently identifying **Claude Code**, **Codex CLI**, and **Grok CLI** as the only vision-capable providers authorized for computer use.
- An explicit exclusion notice explaining that **OpenCode** and other non-vision providers cannot use computer use.
- A live screen vision preview & test tool to inspect capture resolution and verify mouse/keyboard connectivity.

### 2. Provider Restriction Guard
In `bhippi-core::config`, `computer_use.allowed_providers` is locked to `["claude", "codex", "grok"]`. Any configuration attempt to assign computer use to `opencode` or unverified models is rejected at the validator layer (`BhippiConfig::validate`).

In `bhippi-app::chat`, computer use system instructions and execution tools are injected only when `computer_use.enabled` is `true` AND the active provider is in `allowed_providers`. If a user queries computer use while running on OpenCode, the engine explains that OpenCode lacks vision capabilities.

### 3. Screen Vision & Input Automation Subsystem
The engine in `bhippi-app::computer` provides native desktop capture and input synthesis:
- **Vision**: Screen capture extracting viewport bitmaps encoded to standard image formats with coordinate metadata `(width, height)`.
- **Mouse**: `mouse_move`, `mouse_click`, `mouse_drag`, `mouse_scroll`, `get_cursor_position`.
- **Keyboard**: `type_text`, `key_press`, `hotkey` (e.g. `Ctrl+C`, `Win+R`, `Alt+Tab`).
- **Telemetry & Logging**: All computer actions emit events to the Activity Dock so the user has real-time visibility into every desktop action.

## Consequences

- Users can safely enable autonomous PC automation and screen vision directly from Settings.
- OpenCode remains a dedicated text/code agent without failing or misinterpreting visual desktop tasks.
- Architecture rules (INV-003, INV-032, INV-036) remain strictly enforced.
