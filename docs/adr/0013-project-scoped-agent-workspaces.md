# ADR-0013: Project-scoped agent workspaces

Date: 2026-08-26 · Status: accepted · Supersedes: ADR-0012 where conversation scope was only visual

## Context

ADR-0012 added project selection, but conversations remained a single global in-memory list and
provider CLIs still launched from shared `~/.bhippi/workspace`. That allowed project B to display
project A's sessions and gave a coding provider the wrong working directory. The local-folder form
also required a path to be typed rather than opening the operating-system folder selector.

## Decision

The persistent sidebar renders before project selection. Project-only navigation is disabled and
its primary action becomes **Add project** until a project is active.

Every conversation carries its canonical `project_path`. Rust command handlers resolve the active
project from persisted config immediately before list, create, read, delete, send, and regenerate
operations. The engine filters on both conversation id and project path and rejects an id belonging
to another project.

Every completion request carries the same canonical workspace. CLI providers set that exact path
as their child process working directory; other providers receive the boundary through the
versioned `prompts/chat-workspace.md` system context. A missing or replaced directory fails before
the provider call. The official Tauri dialog plugin supplies native directory selection for open,
clone-parent, and create-parent actions, with manual path entry retained as a fallback.

## Security boundary

Bhippi-owned conversation and workspace routing is hard-partitioned in Rust. Bhippi currently has
no agent-facing arbitrary filesystem tool, so no Bhippi tool can address another project.

A third-party coding CLI is launched inside the selected project with a scrubbed environment and
explicit argv, but its vendor process retains its own filesystem/security model. Working-directory
scope and a system instruction are not an operating-system sandbox. The UI states this distinction;
Bhippi must not claim stronger confinement until each provider is launched through a proven OS or
vendor sandbox that blocks absolute-path and symlink escapes.

## Consequences

- Switching projects produces a clean selection and that project's session list only.
- In-memory sessions remain non-durable until `chat_turns` persistence lands, but they cannot cross
  project boundaries during the process lifetime.
- Adding a project establishes a distinct agent workspace without copying or moving user files.
- The dialog plugin is a new app-only dependency and capability; it does not grant general frontend
  filesystem read/write access.

## Alternatives

- **Trust a project path supplied by React:** rejected because frontend state is not a security
  boundary.
- **Use one shared agent directory and mention the project in prose:** rejected because coding CLIs
  discover files and repository state from their working directory.
- **Claim complete OS sandboxing from current-directory confinement:** rejected as technically
  false for third-party executables.
