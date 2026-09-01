# Bhippi Computer Use — implementation plan (v2)

**Status:** in progress
**Scope:** reliable, explicitly requested Windows desktop control from Chat
**Related decision:** ADR-0015
**Supersedes:** v1 of this file, which planned the feature before it had ever run end to end.

## Outcome

When a user clearly asks Bhippi to operate the desktop, an authorised vision-capable provider
receives a fresh screenshot, returns one structured desktop action, Bhippi executes that action
through its own input subsystem, draws the pointer doing it on a live overlay, captures the
resulting screen, and continues until the task is complete or a safety limit stops it. Coding and
ordinary chat requests never activate Computer Use.

---

## Phase 0 — Diagnosis (complete)

Reproduced against the real CLIs installed on this machine. The feature was not "nearly working":
it died before the first action on every provider a user would pick.

| # | Finding | Severity | Evidence |
|---|---|---|---|
| 1 | **Claude never received the prompt.** `--add-dir <directories...>` is variadic, so it swallowed the prompt as another directory. | fatal | Running Bhippi's exact argv returns `Error: Input must be provided either through stdin or as a prompt argument when using --print`. Moving the prompt after the variadic flags returns a normal answer. |
| 2 | **Grok never received the prompt.** `-p, --single <PROMPT>` takes a required value and was immediately followed by `--tools`. | fatal | `grok --help`: `-p, --single <PROMPT>`. Bhippi emitted `-p --tools Read …`, so the flag consumed the next flag instead of the prompt. |
| 3 | The composer's *Computer & Browser Automation* toggle is decorative — `send_chat_message` has no parameter for it. Activation rests entirely on a phrase list. | high | `commands.rs:293` takes no automation flag; the toggle only writes `localStorage`. |
| 4 | The phrase gate blocks itself. Any message containing `build`, `code`, `feature`, `implement` is refused outright, and no phrase covers plain requests like "open Notepad for me". | high | `explicitly_requests_computer_use` returns `false` whenever a development marker appears anywhere in the text. |
| 5 | Every action spawned 3–4 PowerShell processes (bounds → move → click → verify), each recompiling its C# shim. | medium | ~0.5 s per invocation measured; ~2 s of pure overhead per loop step. |
| 6 | The bridge is not DPI-aware, so on a scaled display the screenshot's pixels and the cursor's coordinates disagree. | medium | Latent here (this machine runs 1920×1080 at 100 %); wrong-place clicks on any scaled monitor. |
| 7 | Typing used `SendKeys`, which drops characters under load and cannot emit non-ASCII text. | medium | `type_text` escapes for `SendKeys` and has no unicode path. |
| 8 | Nothing was drawn on the desktop. The user asked to *see* the agent's pointer; only an in-app aura existed. | medium | `ComputerUseAura` renders inside the Bhippi window only. |

**Exit:** met. The failure is explained by evidence from both the chat loop and the OS action path,
and the provider-handoff fix is verified live.

---

## Phase 1 — Provider handoff that cannot lose the prompt

The bug is a class, not an instance: any vendor flag that accepts multiple values will eat whatever
follows it, and the prompt was following it.

- Insert the Computer Use argv fragment **after any leading subcommand and before the vendor's own
  flags**, so the prompt keeps the position the vendor's recipe already proved works.
- Pin the exact argv per provider in unit tests, including a regression test asserting the prompt is
  never the token after a value-taking flag.
- Keep a Windows-only live smoke test that runs the real CLI and asserts a `<computer_action>` block
  comes back.

**Exit:** every authorised provider receives both the prompt and the screenshot, proven by a test
that reads the argv rather than by inspection.

## Phase 2 — Activation the user controls

- Thread an explicit `computer_use` flag from the composer toggle through `send_chat_message` into
  the engine.
- Precedence: toggle on ⇒ authorised; toggle off ⇒ never; unset ⇒ fall back to the phrase gate.
- Rewrite the phrase gate so development discussion is only suppressed when no direct control
  phrase is present, and broaden it to cover ordinary requests ("open Notepad for me").

**Exit:** a user can turn the feature on deliberately, ordinary chat still cannot move the pointer,
and asking Bhippi to fix its own Computer Use code does not trigger it.

## Phase 3 — Windows input that is fast and lands where it aimed

- One PowerShell invocation per action, with a single `Add-Type` block that defines `SendInput` once.
- Declare per-monitor DPI awareness in every bridge call, so bounds, capture and input all speak
  physical pixels.
- Absolute-coordinate mouse movement normalised against the virtual desktop, so multi-monitor and
  negative origins work.
- Unicode typing through `KEYEVENTF_UNICODE` rather than `SendKeys`.
- Cache the desktop bounds for the turn instead of probing before every action.

**Exit:** no console window appears, a click costs one process instead of four, typed text survives
non-ASCII, and coordinates agree with the screenshot on scaled displays.

## Phase 4 — The visible AI pointer

The user's actual request: *add a mouse on the screen and control it.*

- A transparent, always-on-top, click-through overlay window spanning the whole virtual desktop.
- It draws Bhippi's own pointer, glides it to each action's coordinates, and plays a click ripple,
  a drag trail, or a scroll pulse to match the action.
- A caption names the action and the model's stated reason, so the user can watch *why*, not just
  *what*.
- It appears when a Computer Use turn starts and closes when the turn ends or is cancelled.

**Exit:** the pointer is visible on the real desktop for the whole turn and never intercepts a
click.

## Phase 5 — Loop quality

- Carry the model's reason for each action into the overlay caption and the Activity Dock.
- Keep the transcript from growing without bound across a long desktop task.
- Stop on completion, cancellation, execution failure, malformed output, or the action ceiling.

**Exit:** a twenty-step task stays legible and affordable.

## Phase 6 — Gates and live acceptance

- `cargo fmt --check`, `cargo clippy -D warnings`, workspace tests, `tsc --noEmit`, `vite build`,
  architecture tests, generated-IPC freshness.
- Live: ask the running app to open Notepad and type a sentence; watch the overlay pointer do it.

**Exit:** gates green and the observed desktop behaviour matches the checklist below.

---

## Acceptance checklist

- Explicit desktop-control request activates Computer Use; feature-development discussion does not.
- The composer toggle authorises a turn on its own.
- The selected provider receives a real current screenshot **and** the prompt.
- Mouse move, click, drag, vertical/horizontal scroll, typing, key press, and hotkey actions work.
- Every state-changing action requires Full PC Access.
- Bhippi's pointer is visible on the desktop and names what it is doing.
- The user can stop a running turn between actions.
- Action protocol markup is not rendered as assistant prose.
- No visible Command Prompt or PowerShell window appears.
- A failed action stops the loop and produces an actionable error instead of continuing blindly.
- Temporary screenshots are removed after the turn.
