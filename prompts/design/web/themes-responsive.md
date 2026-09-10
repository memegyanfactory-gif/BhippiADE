version: 1
domain: web
title: Themes and responsive
when: three-state theming, breakpoints, the sacrifice order
tags: theme, dark, light, prefers-color-scheme, data-theme, responsive, breakpoint, mobile, tablet, desktop, container-query, viewport, overflow, sacrifice

# Themes and responsive

<!-- section: three-state -->
## 1. Three theme states, one token pattern

The viewer has three states: an explicit `data-theme="dark"`, an explicit
`data-theme="light"`, and "system", which stamps nothing. Structure the CSS so every state
resolves as a set:

```css
:root {                                   /* the complete light palette, every token */
  --bg: #f7f5f0; --surface: #ffffff; --text: #1b1a17; --text-dim: #5c5850;
  --line: #e4e0d7; --accent: #b4562a; --on-accent: #ffffff;
}
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) {       /* system dark, unless the user chose light */
    --bg: #14130f; --surface: #1c1a15; --text: #f1ede4; --text-dim: #a39d90;
    --line: #2a271f; --accent: #e28a5a; --on-accent: #1b1208;
  }
}
:root[data-theme="dark"] {                /* the explicit toggle wins the other way */
  --bg: #14130f; --surface: #1c1a15; --text: #f1ede4; --text-dim: #a39d90;
  --line: #2a271f; --accent: #e28a5a; --on-accent: #1b1208;
}
body { background: var(--bg); color: var(--text); }
```

Rules: every token exists on the bare `:root`; components use tokens and never sit inside a
media or `[data-theme]` block; `body` paints its own background from a token because the
host paints its own ground behind the page; the dark set is designed, not inverted — the
accent may need a lighter step, saturated fills drop a notch, shadows soften. A single-theme
design skips the media query and the stamps and still paints every colour explicitly.

<!-- section: breakpoints -->
## 2. Breakpoints

`640 · 900 · 1200 · 1600`. Mobile-first or desktop-first is decided by the audience, once.
Use container queries for components that live in more than one column width.

<!-- section: sacrifice-order -->
## 3. The sacrifice order

What is *sacrificed* at each step is a design decision, not an accident. Decide the order
things drop out before writing the media query: the right panel collapses first, then the
rail becomes icons, then secondary columns stack, then the table becomes a list, then
decorative imagery goes. The primary action never drops. Headlines wrap; they do not shrink
below the scale's step.

<!-- section: units -->
## 4. Units

Relative units for type and spacing (`rem`, `em`, `ch` for measure); `px` for hairlines and
radii; `dvh` not `vh` for full-height on mobile; `max-width: 100%` on every image; wide
content scrolls inside its own `overflow-x: auto` box — the page body never scrolls
sideways.

<!-- section: touch -->
## 5. Touch and pointer

Hit targets ≥ 44 px on touch; hover states have a non-hover equivalent (a visible label, a
tap); no fake device chrome in mockups. Test at 375 px wide and at 200 % zoom.
