# ADR-0023: Multi-session project workspace

Date: 2026-08-30 · Status: accepted · Supersedes: ADR-0010/0014 single-session presentation only

## Context

The project rail already groups chats and CLI sessions under their canonical project, but the main
workspace renders only the selected session. Operators working with several agents or terminals in
one project must repeatedly switch context and cannot compare progress on one screen. Project card
ordering also has no protected position: a deliberate drag can move every card.

## Decision

- The Agent screen has a persistent **Single / Multi** presentation switch. Single preserves the
  existing one-session surface. Multi renders every chat and CLI session whose canonical
  `project_path` matches the active project.
- Dragging one Single-mode session tab onto another is the direct compare gesture: the target shows
  a labelled merge state and the workspace switches to Multi on drop. Overflowing tab and panel
  rails support horizontal trackpad/wheel movement with proximity snapping rather than tiny targets.
- Multi mode owns an **Organize** control with auto-fit plus Balanced columns, Adaptive tidy, and
  Smart fit layouts. With three or four windows, Smart fit keeps the first explicitly ordered
  window in a full-height primary column and stacks every remaining window evenly in the
  secondary column. Focus, typing, and turn activity never change panel positions; only an
  explicit drag reorder can choose a different primary window. Layout choice and presentation-only
  panel sizes are local UI preferences.
- Panels can be widened by pointer or keyboard. Manual resizing holds a 300 px floor and an 85%
  workspace maximum; auto-fit may compress secondary panels to 260 px before the rail scrolls.
  Narrow workspaces stack panels at full width.
- Every mounted chat filters engine events by owned turn id. A permission, stream delta, phase, or
  completion event from one session must never mutate or answer another visible session.
- Project cards remain draggable. Users can pin a project into a stable top group; pinned cards
  cannot be displaced until unpinned. Ordering, pins, minimized cards, workspace mode, layouts, and
  panel sizes persist in local storage because they are presentation preferences, not project data.
- Canonical project selection, conversation ownership, CLI working directories, IPC types, and all
  cross-project rejection rules remain Rust-owned and unchanged.

## Consequences

Several complete chat surfaces may be mounted at once, increasing webview memory in proportion to
the active project's session count. The manual 300 px control floor means sufficiently large sets scroll
horizontally instead of shrinking into unusable tiles. No dependency, IPC command, database table, or
business rule is added. Keyboard users can toggle modes, choose layouts, resize panels, pin cards,
and focus a panel in Single mode.

## Alternatives

- **Tabs only:** rejected because it preserves the context switching the feature is meant to remove.
- **Unlimited freeform canvas:** rejected because overlap and arbitrary shrinking break the dense
  chat controls and make recovery difficult.
- **Persist layout in the database:** rejected because panel arrangement is device-local chrome,
  not durable project or research state.
