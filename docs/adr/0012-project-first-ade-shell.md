# ADR-0012: Project-first ADE shell

Date: 2026-08-26 · Status: accepted · Supersedes: ADR-0006 and `04-PAGES.md` A0/A1b for desktop entry and chat framing

## Context

The original desktop shell opened directly into a general conversation surface. The owner has
changed the product direction: Bhippi must behave as an agentic development environment (ADE)
whose sessions belong to an explicitly selected project. A fresh install must not imply a
workspace, repository, branch, editor integration, or remote connection that the user did not
choose.

## Decision

Bhippi starts behind a project gate. The user may attach an existing directory, clone an explicit
Git URL, or create a new directory. Project references and the active project persist in
`config.toml`; forgetting a reference never deletes filesystem content.

Once a project is active, the persistent sidebar exposes Agent, Research, Automation, Library,
and project sessions. A compact header identifies the real project path and Git state and offers
explicit-argv launch actions for detected editors and the system file manager. The conversation
surface is labelled Agent. A new session begins with its composer in the workspace centre; after
the first instruction it docks to the bottom for the lifetime of that session.

Filesystem validation, Git operations, tool detection, and process launch live in Rust and cross
generated Specta IPC. The TypeScript surface renders state and accepts input only.

## Consequences

- Fresh launch is clean and truthful, but one project selection is required before agent work.
- Existing in-process conversations are not yet database-persisted or project-keyed; the active
  project frames them until the planned `chat_turns` persistence ticket adds a project foreign key.
- Editor launchers must be on `PATH` to be enabled. Unavailable tools remain visible and explained.
- Project removal is deliberately non-destructive. Directory deletion is outside this command set.
- The technology/AI domain lock and every existing research/publishing gate remain unchanged.

## Alternatives

- **Infer the current repository from Bhippi's process directory:** rejected because packaged apps
  have an implementation working directory, not a user-selected project.
- **Keep a chat-first home and add project chips:** rejected because it preserves the wrong mental
  model and allows sessions with ambiguous scope.
- **Perform filesystem and launcher work in TypeScript:** rejected by R3 and because it would weaken
  path validation and explicit-argv guarantees.
