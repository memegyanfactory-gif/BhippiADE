# ADR-0010: Sidebar chrome — navigation leaves the title bar, the ticker moves into Research

Date: 2026-08-26 · Status: accepted · Supersedes: 04-PAGES §A0 diagram, §A0.1 location, §A0.2, §A1b region 1 · Amends: 04-PAGES (same change)

## Context

The persistent chrome was four stacked rows: ticker (36 px) · title bar with the four screen
tabs (44 px) · screen · status bar. Two pressures landed together.

**The reference layout.** The owner's UI revision brief (`09-UI-REVISION-BRIEF.md` §W4)
asks for a left sidebar carrying navigation, the conversation list, and an account row, with
the top tab bar gone. A horizontal tab strip above a vertical sidebar is redundant chrome:
two places answering "where am I".

**The ticker was in the wrong place.** It renders vendor headlines that exist to be turned
into research runs — but it sat above every screen, including Automation and Settings, where
there is nothing to run a story against. It cost 36 px of every screen and served one.

## Decision

1. **One sidebar owns navigation and conversations.** ~280 px, `--surface`, hairline right
   border: a compact icon row (collapse, filter, back, forward), a quiet `+ New` button, four
   icon+label nav rows (active = inset accent rail), a `Conversations` section header, the
   conversation list, and a hairline-separated account row at the bottom. Collapse keeps a
   48 px icon rail so navigation never disappears.
2. **The title bar slims to 40 px**: wordmark (+ `demo` badge), settings gear, window
   controls. Nothing else. The running indicator moved into the status bar, next to the
   provider it describes.
3. **`TickerStrip` moves into the Research screen**, above the pane, out of the scroll flow.
   The component, its states and its data contract are unchanged — only its position. When
   research automation lands (S9) it can earn other locations; until then one home.
4. **Chat loses its private rail.** Conversation state lifted from the Chat screen into the
   app shell so the list survives switching screens; Chat receives it as props. The
   no-conversation face becomes a left-aligned greeting, recent sessions as hairline rows,
   then the existing suggestion chips.

Screen history gives the back/forward pair real behaviour; `/` focuses the conversation
filter. No new dependencies, screens or seams — this rearranges documented furniture.

## Consequences

- Easier: one place answers "where am I" and "what did I open"; the conversation list stops
  resetting when a user peeks at Research.
- Easier: Research gains 36 px of vertical space exactly where the tier picker lives.
- Harder: `04-PAGES §A1b` region 1 no longer describes a chat-local rail; the doc is amended
  in this same change and future chat work must target the shell sidebar instead.
- Harder: the empty chat face now renders recent sessions, so it depends on the conversation
  list loading; its *loading* state says so rather than rendering an empty void.

## Alternatives rejected

- **Keep tabs and add a sidebar.** Two navigation systems; rejected as redundancy, not
  caution.
- **Delete the ticker.** It is a real feature (spec §15.3); hiding a feature because it sits
  badly is how features die. Moved, not removed.
- **Sidebar inside each screen.** Per-screen rails are why conversation state reset on
  screen switches in the first place; the list must outlive the screen that shows it.
