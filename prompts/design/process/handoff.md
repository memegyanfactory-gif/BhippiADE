version: 1
domain: process
title: The handoff spec
when: the spec a design needs before it becomes a batch or a page
tags: handoff, spec, tokens, component, props, states, breakpoints, edge-cases, animation, implementation, batch

# The handoff spec

A design becomes code through a spec, not a description. Before a page is written or a batch
is emitted, the spec answers these; a missing answer is a guess you will make in code, badly.

<!-- section: tokens -->
## 1. Tokens

Every colour, size, radius, duration and font as a named token with its value — the plan's
values, resolved. On the web: the `:root` block for all three theme states. In Godot: the
`Theme` resource's colours, fonts, font sizes, constants and styleboxes, and which
`theme_type` each belongs to.

<!-- section: layout -->
## 2. Layout

The shell and its regions with their sizes and behaviour at each breakpoint (or, for a HUD,
each anchor and its safe-area inset; for a scene, the metric grid and the focal camera).
What drops out first when space shrinks.

<!-- section: components -->
## 3. Components

Per component: its anatomy (parts, in order), its props or exported variables, its variants,
and every state — rest, hover, active, focus-visible, disabled, loading, error, selected — with
the token each state uses. A component with a missing state is a bug that ships.

<!-- section: interaction -->
## 4. Interaction

Keyboard and gamepad paths; focus order; what Escape / Back and Enter / A do on each surface;
which actions confirm and which undo; hit target sizes.

<!-- section: edge-cases -->
## 5. Edge cases

Long names, zero items, a thousand items, a 200 % font scale, a 4:3 and a 21:9 screen, no
network, a missing asset. Each has a rendered answer.

<!-- section: animation -->
## 6. Animation

Each moving thing: trigger, duration token, easing, property, and its reduced-motion form.

<!-- section: batch-shape -->
## 7. For a Godot batch

The spec lowers to typed actions: the `Theme` as a resource written through the typed path,
`Control` nodes with anchors and containers, `theme_override_*` only for the documented
exception, scripts through `write_script`. Name the scene, the parent paths and the node
names before the batch, so the batch is one transaction the user can undo as one thing.
