# ADR-0014 — In-app workbench (editor + browser) and the activity dock

- **Status:** Accepted
- **Date:** 2026-08-26
- **Supersedes:** nothing
- **Relates to:** ADR-0012 (project-first ADE shell), ADR-0013 (project-scoped workspaces)

## Context

The ADE shell (ADR-0012) gave Bhippi a project, a session list, and an agent that works
inside a canonical project directory (ADR-0013). What it did not give the user was any
way to *see* the result without leaving the app: to read a file the agent had just
written, or to run whatever it had just built. Both required switching to VS Code and a
browser, at which point Bhippi is a chat window that happens to know a folder path.

Three problems had to be settled together:

1. **Where do the editor and the browser live**, without turning the conversation into a
   sliver.
2. **What may the in-app browser load.** An embedded frame in a desktop app has no
   address bar of its own and no origin indicator the user can trust.
3. **How does the user know what the agent is doing**, and where does the agent ask when
   it needs an answer before it can continue.

## Decision

### 1. One workbench pane, two modes, closed by default

The editor and the browser are two modes of a single right-hand pane, not two screens
and not two panes. They share one frame, one width, and one closed/open state.

The pane is **closed on every launch**. It is not a remembered preference, because an
editor and a browser that open themselves take two thirds of the window from the
conversation the user actually came for. The pane's *width* and its *last mode* are
remembered, so reopening lands where it was.

Both panes stay mounted once opened. Unmounting the browser would tear down the frame and
reload the dev server on every toggle; unmounting the editor would discard an unsaved
buffer. The browser does not mount at all until first requested, so an unopened browser
never probes a port.

### 2. The in-app browser loads loopback only

`localhost`, `127.0.0.1`, `0.0.0.0`, and `::1` over http/https. Everything else is
refused with an explanation and offered to the system browser instead.

This is a security decision, not a scope decision. An embedded frame that will load any
URL is a browser with no address-origin indicator inside a trusted desktop app — the
exact shape of a credential-phishing surface, and one the user has no way to inspect.
Handing a non-local URL to the real browser puts it somewhere the address bar is visible.

Navigation history is kept by the pane rather than read from the frame. A cross-origin
frame will not report where it navigated, and back/forward buttons driven by a guess
would lie about state the user is relying on.

Reachability is measured, never assumed: `preview_targets` marks a port reachable only
when a TCP connection to it succeeded during that call. `package.json` is read solely to
*label* an idle port with the command that would start something on it.

### 3. A hand-rolled tokenizer, not a highlighting dependency

The editor colours comments, strings, numbers, keywords, types, and calls, per line, from
a ~250-line tokenizer. It is a tokenizer and not a parser, and it gets ambiguous cases
deliberately wrong-but-harmless by leaving them in the default colour.

A real grammar engine is megabytes in a desktop bundle for a pane whose job is to glance
at a file the agent just wrote. Per-line tokenizing also means the cost scales with the
visible window rather than the buffer.

The tokenizer returns **spans, never an HTML string**. Nothing is inserted with
`innerHTML`, so a project file containing `<script>` is text, not markup.

### 4. Filesystem access stays in Rust, confined to the active project

Six commands (`list_workspace_dir`, `read_workspace_file`, `write_workspace_file`,
`preview_targets`, `read_project_rules`, `write_project_rules`) do all the walking,
skipping, ordering, and probing. TypeScript receives already-decided rows (R3).

Confinement is two-layered. `sanitize_relative` rejects `..`, drive prefixes, and control
characters *before* the filesystem is touched, so a hostile string never becomes a
`canonicalize` call. `resolve` then canonicalizes the target and checks
`starts_with(project_root)` — running the check on the canonical result is what makes a
symlink pointing outside the project fail rather than quietly open another project's file.

The active project comes from Rust state (`required_project_path`), never from a
frontend-supplied path, exactly as ADR-0013 requires for chat.

### 5. Project rules live in the project

Standing agent instructions are `.bhippi/rules.md` **inside the project folder**, not in
`~/.bhippi/config.toml`. They travel with the repository, can be committed and reviewed,
can be edited outside Bhippi, and switching projects genuinely switches rules instead of
carrying one project's conventions into another.

`prompts/chat-rules.md` (INV-035, `version: 1`) wraps them into the system prompt after
the workspace boundary statement and before the effort directive, and states plainly that
rules never widen access or override the boundary, the technology/AI scope, or any safety
rule. Content is truncated at 8 000 characters — standing instructions are a page, not a
corpus.

### 6. The activity dock is the one place the agent reports and asks

A dock above the composer shows what the engine is doing: collapsed, one breathing line;
opened, every step with its icon, state, and elapsed time.

**The agent's questions land in the same panel.** A permission request opens the dock
itself and renders Allow / Deny inline. The thing you are watching and the thing being
asked of you belong in one place — a question that arrives further up a scrolling thread
arrives somewhere the user is not looking.

Every row comes from an event the backend actually emitted (`chat-tool`,
`chat-thinking`, `chat-permission-requested`). Nothing is inferred and no step is shown
that the engine did not report. A panel that invents plausible-looking activity is worse
than no panel, because it is the one surface a user trusts to know what is really
happening.

## Consequences

**Good.** A change the agent makes can be read and run without leaving Bhippi. The
browser cannot be pointed at a remote origin from inside the frame. The file surface is
provably confined to the open project, with tests for traversal, drive prefixes, control
characters, and symlink escape. Rules are versioned with the project that owns them. The
user can always see what is running and is asked in the place they are already watching.

**Costs accepted.** Highlighting is approximate — it will mis-colour a regex containing a
quote, and it does not know types from identifiers beyond a capital letter. The editor is
a reading-and-small-edits surface, not a replacement for VS Code, and "Open in…" remains
the answer for real work. The browser cannot preview a deployed site; that is deliberate.
Port probing covers nine common defaults, so an unusual port must be typed by hand.

**Revisit if.** The editor grows multi-file tabs, search, or a language server — at that
point a real editor component earns its bundle size. Or if a genuine need appears to
preview a non-local URL, which would require a real origin indicator in the pane's chrome
first, not a relaxation of the loopback rule.
