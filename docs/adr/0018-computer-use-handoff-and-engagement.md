# ADR-0018 — Computer Use self-engagement and vision-provider handoff

- **Status:** Accepted
- **Date:** 2026-08-27
- **Supersedes:** nothing
- **Relates to:** ADR-0015 (computer use & vision automation), ADR-0006 (chat surface), ADR-0008 (provider edges)

## Context

ADR-0015 gated Computer Use to vision-capable providers (claude/codex/grok) and injected the
desktop controller only when the *active* picker provider was in `allowed_providers`. In practice
that left two real failure modes:

1. **Silent disengagement.** The intent gate recognised a narrow phrase list, so natural requests
   ("move the mouse to the centre of the screen", "double-click the file on my desktop") fell back
   to an ordinary text answer and the desktop never engaged — read by users as "nothing happens".
2. **Dead ends instead of help.** When Computer Use was explicitly requested but the *selected*
   provider could not see the desktop (e.g. the chat was on OpenCode or a local text model), the
   engine only appended an explanation to the system prompt. No work happened, and there was no
   automatic path to the very providers ADR-0015 already deemed authorised.

Separately, the desktop-loop prompt underspecified the coordinate mapping between the screenshot
(which is a 1:1 pixel image of the whole virtual desktop) and absolute virtual-desktop coordinates,
which caused off-by-origin mouse positions on multi-monitor setups.

## Decision

### 1. Wider, still-conservative intent gate

`computer::explicitly_requests_computer_use` keeps its development-discussion veto (requests to
*build, fix, or debug* computer use are never treated as permission to use the desktop) but now
accepts a second, tight heuristic: an explicit desktop-control verb ("click", "double-click",
"scroll", "drag", "type", "press", "move/use/control the mouse/cursor") *joined* to a desktop
object ("screen", "desktop", "on my computer/pc", "taskbar", "start menu", "file explorer",
"notepad", "calculator"). Both parts must be present; a message with only one is not a trigger.

The explicit `/computer <task>` command bypasses the natural-language heuristic and its
development-discussion veto. The command is therefore a deterministic per-turn override, while
plain language remains conservative. Direct requests to "access my computer/PC" are also
recognised as natural Computer Use intent.

### 2. Auto-handoff to an authorised vision backend

When Computer Use is explicitly requested, `computer_use.enabled`, and the selected provider is
**not** vision-ready (not in `allowed_providers`, or its model is text-only), `run_turn` now hands
the session to the first enabled, installed, authorised vision CLI — `claude`, then `codex`, then
`grok`, skipping the current picker choice — before answering. If none is available, the previous
explanatory messaging returns unchanged.

The swap is never silent: it emits its own thinking line ("Computer Use handed to … for desktop
vision"), tells the handed-off provider in the system context that it is now the session driver,
carries a `Session note` into the request, and reproduces the note in the transcript's final text
so the record shows which backend actually ran.

### 3. Coordinate contract in the loop prompt

`prompts/chat-computer-use.md` (v3) states that the screenshot is a 1:1 pixel map of the whole
virtual desktop, that observed `Origin (X, Y)  Size W×H` bounds translate image pixel `(px, py)`
to absolute desktop coordinate `(X+px, Y+py)`, and that negative multi-monitor origins are valid
(never clamped to zero). It also requires exactly one action per response, plain-text completion
summaries with no trailing action block, and agrees that multi-step tasks take multiple
single-action iterations with a fresh screenshot between each.

## Consequences

- Users get working pointer control from natural phrasing and from whichever provider they happen
  to be chatting on — no more silent "nothing happened" turns.
- The provider set is unchanged: the handoff only ever picks claude/codex/grok, so ADR-0015's
  exclusion of OpenCode and text-only locals still holds as a hard minimum.
- The honest transcript line plus the driver note preserve visibility, the Activity Dock's
  per-action cards already show every executed input.
- Live verification lives in `crates/bhippi-app/tests/computer_loop_live.rs`: a deterministic
  synthetic-vision agent completes the full loop on the real desktop (capture → observation →
  tag → validation → execution → pointer-landing assertion), and a real-CLI variant spawns the
  actual vendor executable (skipping only when the account is exhausted).
