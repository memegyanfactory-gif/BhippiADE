# ADR-0017 — Local model servers are detected, never started, and released on switch

- **Status:** Accepted
- **Date:** 2026-08-27
- **Amends:** ADR-0006 (§ silent default), ADR-0008 (provider edges)
- **Relates to:** ADR-0016 (typed faults)

## Context

Opening Bhippi opened Bionic. Every time.

Three separate defects stacked into that, and each one is worth naming because they fail
differently:

**1 · Detection executed the binary.** `detect::server_row` probed a local server's ports,
and when none answered it fell through to `read_version(binary)` — which runs
`<binary> --version`. A local LLM server is a *desktop application*: it does not implement
`--version`, so that call does not read a version, it **launches the program**. Detection
runs on every sweep, so every launch of Bhippi launched Bionic's full UI and loaded a model
into RAM nobody had asked for. Measured on the reporting user's machine: five `Bionic.exe`
processes holding roughly 650 MB, none of them requested.

**2 · Presence was reported as health.** That same fallback returned
`Health::Healthy { latency_ms: 0 }` and `installed: true` for a server that had answered
nothing on any port. A server that is not accepting connections cannot answer a prompt, and
calling it healthy is simply false.

**3 · Bionic was hardcoded first in the default order.** `from_detection` ranked
`["bionic", "lmstudio", "ollama", …]` and filtered on `installed`. Combined with (2), a
stopped Bionic won the default pick over a working, signed-in cloud backend.

## Decision

### 1 · A server is detected by listening, and by nothing else

The version probe is removed from the local-server path entirely. Nothing in detection may
execute a `LocalServer` binary, ever.

Presence on disk is still reported — as `offered: true` with
`Health::Unavailable { reason: "installed, but not running — start it to use it" }`. That is
a third state, distinct from both "running" and "absent", and it needs to be, because the
fix differs: start it, versus install it.

**Bhippi does not start it.** That is the whole point of this ADR. A model server is a
multi-gigabyte residency decision that belongs to the user.

### 2 · `usable()` replaces `installed` as the readiness test

```
LocalServer → listening on a probed port AND healthy
Cli, CloudApi → present
Demo → always
```

`ProviderRuntime::from_detection` filters on `usable()`, so an idle server is not in the
picker, is not resolvable, and cannot be the default.

### 3 · Default preference: running-local → ready-remote → demo

1. A local server **already running** — free, private, and already holding the memory, so
   using it costs nothing more.
2. Otherwise a cloud API or CLI that is ready. Nothing local is started to get here.
3. The offline demo.

Step 2 **amends ADR-0006**, which kept CLIs out of the silent default because a signed-out
one failed confusingly. That reason expired with ADR-0016: a signed-out CLI now renders a
fault card naming the exact `login` command, so falling back to one is helpful rather than
mysterious. The user asked for this fallback explicitly.

### 4 · Switching away releases the model

`set_active_provider` asks the server the user just left to unload. What is achievable
differs per backend, and is reported honestly rather than assumed:

| Backend | Outcome |
|---|---|
| Ollama | `keep_alive: 0` — documented, reliable, actually frees the memory |
| LM Studio | Newer builds expose an unload; older ones do not, and are told so |
| llama.cpp · vLLM · Jan · text-generation-webui · Bionic | No unload exists in the surface they expose. Reported as `NotSupported` with the action that *would* work |

The preference is saved **before** the eject, so an unreachable server can never block a
switch away from itself. Ejection is a courtesy to the user's RAM, never a precondition.

### 5 · Port probing is concurrent

The candidate ports for one server were probed serially at a 2 s budget each — up to twenty
seconds of app start spent discovering that nothing is running. Loopback probes share no
state, so they now run concurrently and the whole sweep costs about one timeout. Measured:
the detection test suite went from **22.09 s to 2.05 s**. Candidate lists were also trimmed
to the ports each vendor is actually seen on; a server on a genuinely custom port is
configured in Settings, not guessed at more widely.

## Consequences

**Good.** Opening the app opens nothing. RAM is the user's to spend. A running local server
is still preferred automatically, which is what someone who has one wants. Startup detection
is roughly ten times faster.

**Costs.**

- A user who *wants* Bhippi to launch their local server no longer gets that. It is not
  offered, deliberately — the failure mode of guessing wrong is gigabytes of surprise.
- Three of the six local backends cannot be told to unload. Saying so plainly is the only
  honest option; the alternative is claiming a success that did not happen.
- `usable()` is a behaviour change for anything that previously read `installed` on a local
  server row. That is the bug, but it is a change.

## Verification

With Bionic closed, the desktop build was launched and left running for forty seconds.
`tasklist` reported **zero** `Bionic.exe` processes throughout, the log recorded
`local server not detected on any port` for all six local backends, and the runtime
resolved `"default":"claude"` — an online backend, chosen automatically, with no local
model loaded.
