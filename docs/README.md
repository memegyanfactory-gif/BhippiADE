# Bhippi — Documentation Set

Bhippi is a desktop application that researches technology and AI to a controllable depth,
builds a persistent knowledge graph from what it learns, and publishes SEO-optimised,
image-rich posts to a static site — manually, on a timer, or reactively from a live news
ticker.

This folder is the project's working memory. **Read it in order the first time.**

---

## Reading order

| # | Doc | What it answers | Read when |
|---|---|---|---|
| 0 | [`00-SPEC-v1.0.md`](00-SPEC-v1.0.md) | What are we building and why? Product intent and every HARD REQ | first, in full |
| 1 | [`01-ARCHITECTURE.md`](01-ARCHITECTURE.md) | How is it structured? Layers, crate graph, state machine, events, errors, persistence, seams | before any code |
| 2 | [`02-MODULE-CONTRACTS.md`](02-MODULE-CONTRACTS.md) | What is my crate's API and what must it guarantee? | before touching a crate |
| 3 | [`03-DATA-MODEL.md`](03-DATA-MODEL.md) | Tables, indexes, blob store, repositories, doctor | before a migration or a query |
| 4 | [`04-PAGES.md`](04-PAGES.md) | Every app screen and every published blog page, with all four states | before UI or theme work |
| 5 | [`05-PIPELINES.md`](05-PIPELINES.md) | The end-to-end flows, step by step, with gates | while implementing a stage |
| 6 | [`06-INVARIANTS.md`](06-INVARIANTS.md) | The rules that cannot be broken, and where each is enforced | before every PR |
| 7 | [`07-AGENT-GUIDE.md`](07-AGENT-GUIDE.md) | **How to work in this repo** — the operating contract for agents | every session, first |
| 8 | [`08-BUILD-ORDER.md`](08-BUILD-ORDER.md) | What to build next and in what order | when picking up work |
| — | [`PROGRESS.md`](PROGRESS.md) | Where the build actually is right now | every session, first |
| — | [`adr/`](adr/) | Why a decision changed | when something surprises you |

**If you are an AI agent picking this project up: start with `07-AGENT-GUIDE.md`, then
`PROGRESS.md`. Those two tell you what to do. The rest tells you how.**

---

## The shape of the system in one screen

```
UI (React, renders only)
   ↓ Tauri IPC (types generated from Rust)
ORCHESTRATOR  bhippi-core — session FSM, queue, budgets, kill switch, event bus
   ↓
CAPABILITIES  research · harvest · memory · vision · writer · seo · ticker · skills · publish
   ↓
PLATFORM      bhippi-providers (any LLM: CLI, API, local)  ·  bhippi-db (SQLite + vec + FTS)
   ↓
FOUNDATION    bhippi-types
```

```
seed topic → PLAN → EXPAND (best-first over a frontier) → HARVEST → DOTS
                ↑                                            ↓
                └────── frontier scoring ←──── MIND MAP ─────┘
                                                  ↓
             SYNTHESISE → FACT GATE → WRITE → IMAGES → SEO → VERIFY → PUBLISH
                                                  ↓
                                         GIST → MEMORY (next run starts smarter)
```

---

## The five things that define this product

1. **Domain lock.** Technology and AI only. A topical classifier rejects everything else at
   ingestion, with no "close enough".
2. **Local-first.** A full research-to-publish run must complete offline, on a local model,
   with zero API keys. Cloud is an accelerator, never a dependency.
3. **Everything is inspectable.** Every published sentence traces to a dot, to a source, to a
   URL — in one click.
4. **The depth ladder is a contract.** X2 / X6 / X12 / X24 are budgets with hard ceilings and
   quality floors, not vibes.
5. **Never publish what you cannot defend.** Eight editorial gates block publication in code.
   Thin evidence is held for review, always.

---

## Change control

- Spec intent changes → update `00-SPEC-v1.0.md` and bump its version.
- Structural changes → write an ADR in `adr/`, then update the affected doc in the same
  change.
- Never deviate silently. A silent deviation is the most expensive thing anyone can do here.

---

## Not part of this project

`C:\Work\VSCode\Bhippi` contains an unrelated static site ("Bhippi +", an app-discovery
landing page). It is not the blog target, not the theme, and not a dependency. Do not modify
it as part of this work.
