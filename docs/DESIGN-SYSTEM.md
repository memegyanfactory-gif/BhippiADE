# The Bhippi Design System

Version 1.1 · 2026-08-29 · Authority: below `00-SPEC` and ADR-0009 (accent) / ADR-0016 (motion).

> **v1.1 — picker & speed polish:** model picker now surfaces OpenRouter `:free` models with a `FREE` pill, supports `/free` filtering, and pins favourites (★) to the top via `localStorage:bhippi_fav_models`; effort/speed panel refined to token-driven hairlines, subdued warp and token-based spent fills; drop-up and composer cleaned for density. The prompt directive in `bhippi-app::chat::DesignMode` was rewritten to be self-contained and checklist-driven so the skill survives without reading this file.

This is the system every surface Bhippi builds is held to — its own screens, and any UI it
writes for a user. It is written to be *applied*, not admired: every rule states the
decision it makes and the failure it prevents, because a design system whose rules cannot
be argued with is a mood board.

---

## 0 · The five judgements

Everything below serves these. When a rule and a judgement conflict, the judgement wins.

1. **Density is a feature.** This is a tool for people who use it for hours. Generous
   whitespace reads as "designed" in a screenshot and as "scrolling" on the fourth hour.
   Default to 13px text on a 4px grid, and spend space only where scanning breaks down.
2. **One accent, used sparingly.** A second accent colour does not add emphasis, it removes
   it — when two things are highlighted, neither is. The accent marks the one action that
   matters on a screen. Everything else is neutral.
3. **Hairlines over shadows, in the layout.** A shadow on a surface that sits *in* the page
   invents a light source the flat surfaces around it contradict. Shadows are reserved for
   things that float *over* the page, where lift is the actual message.
4. **Motion must mean something.** Every animation answers "what changed, and where did it
   come from". Decoration that survives a second viewing is rare; decoration that survives
   the four-hundredth is rarer still.
5. **State is never colour alone.** Every state carries a second signal — a glyph, a label,
   a position. Colour-blind users are 8% of men, and every user is colour-blind in bright
   sunlight.

---

## 1 · Foundations

### 1.1 Colour

A palette is **one accent over one neutral ramp**, plus semantic colours that are never
used decoratively.

**The neutral ramp** — seven steps, one temperature. Warm neutrals (a trace of yellow/red)
read as calm and paper-like; cool neutrals (a trace of blue) read as technical and clinical.
Pick one and never mix: a warm surface beside a cool one looks like a rendering bug.

| Token | Role | Rule |
|---|---|---|
| `--bg` | The page behind everything | Darkest (dark theme) or lightest (light) |
| `--surface` | Panels, cards, the composer | One step from `--bg` |
| `--surface-2` | Recessed wells, inputs, hover | One step again |
| `--surface-3` | Pressed, selected, active | The last step that is still neutral |
| `--line` | Every hairline | Must be visible against both `--surface` and `--surface-2` |
| `--line-strong` | Emphasis borders, focus | ~2× the contrast of `--line` |
| `--text` / `--text-dim` / `--text-faint` | Primary / secondary / tertiary | 4.5:1 · 4.5:1 · 3:1 minimum |

**The accent** — one hue, four tokens: `--accent` (the colour), `--accent-hi` (hover),
`--accent-dim` (a wash at ~12–18% for backgrounds), `--accent-line` (a border at ~35–45%),
and `--on-accent` (text *on* the accent, which must itself clear 4.5:1).

**Semantic colours** — `--ok`, `--warn`, `--error`. These mean exactly one thing each and
are never borrowed for emphasis. If a button is red, it deletes something.

**Contrast floors, non-negotiable:** body text 4.5:1 · large text and UI glyphs 3:1 · a
focus indicator 3:1 against both the component and the page behind it.

**Chart colours are a separate scale.** Categorical series are assigned by identity, never
by rank, and never drawn from the accent — a chart tinted with the brand colour implies the
brand *is* one of the series.

### 1.2 Type

One family for prose, one for code. Two families is a system; three is a ransom note.

| Token | px | Use |
|---|---|---|
| `--fs-micro` | 10 | Keyboard hints, badges |
| `--fs-xs` | 11 | Metadata, timestamps |
| `--fs-sm` | 12 | Secondary UI, captions |
| `--fs-base` | 13 | Everything by default |
| `--fs-md` | 15 | Section headings |
| `--fs-lg` | 18 | Screen titles |
| `--fs-xl` | 24 | The one hero line per screen |

Weights: 400 body · 550 emphasis · 600 headings. Nothing heavier — 700 at 13px is a
smudge. Line height 1.5 for prose, 1.2 for headings, 1.6 for code.

Numbers that change in place — timers, counters, token counts — use `font-variant-numeric:
tabular-nums`, or the whole row shifts on every tick.

### 1.3 Space

A 4px grid: `4 · 8 · 12 · 16 · 24 · 32 · 48`. Nothing between. The grid is what makes
unrelated components line up without anyone coordinating.

**Proximity does the grouping.** Related things sit 4–8px apart, groups 16–24px apart.
A border between two things that are already 24px apart is a border doing nothing.

### 1.4 Shape

`--radius: 4px` for controls, `--radius-lg: 8px` for panels, `--radius-modal: 6px`, and
`999px` for pills. A radius should be *smaller* on smaller elements — a 12px radius on a
24px button is a lozenge.

Concentric rule: an inner radius equals the outer radius minus the padding between them.
Equal radii on nested boxes look wrong for a reason people cannot name.

### 1.5 Elevation

Four levels, and only for surfaces that float over the page:

| Level | Use |
|---|---|
| flat | Anything in the layout. Hairline only. |
| `--lift-1` | Hover on a card, a sticky bar |
| `--lift-2` | Drop-ups, menus, popovers |
| `--lift-3` | Modals |

Light themes soften all three together — a dark-theme shadow on a light ground reads as
dirt, not depth.

### 1.6 Motion

From ADR-0016. Durations are named for the *kind* of change:

| Token | ms | Change |
|---|---|---|
| `--t-instant` | 90 | Press / release |
| `--t-quick` | 140 | Hover, focus, small flips |
| `--t-move` | 220 | A panel travels |
| `--t-enter` | 300 | Something arrives |
| `--t-settle` | 420 | Arrives and comes to rest |
| `--t-ambient` | 1600 | "This is alive" loop |

Easings: `--e-out` for arrivals, `--e-in` for departures, `--e-both` for anything that moves
and stops, `--e-spring` **only** for a confirmation, `--e-linear` only for a continuous loop.

**Transform and opacity only.** Animating `width`, `height`, `top`, or `margin` forces
layout on every frame.

**Stagger** siblings by 34ms, capped at ~12 — past that the last row waits half a second
for a reason nobody perceives.

---

## 2 · Components

Each entry: what it is for, its anatomy, and the rule that is most often broken.

### 2.1 Buttons

Five variants, and the choice between them is about *consequence*, not appearance.

| Variant | For | Per screen |
|---|---|---|
| **Primary** | The one action the screen exists for | At most one |
| **Secondary** | Real actions that are not the main one | Any number |
| **Ghost** | Tertiary actions, toolbars | Any number |
| **Danger** | Destroys something | At most one, never pre-focused |
| **Link** | Navigates rather than acts | Any number |

Sizes: `sm` 24px · `md` 28px · `lg` 34px. Padding is `0 12px` at `md`; a button narrower
than 64px reads as an icon that failed to load.

Every button has all six states: rest, hover, active, focus-visible, disabled, loading. A
loading button keeps its width — a button that shrinks to a spinner moves everything after it.

**Most-broken rule:** a disabled button with no explanation. If it is disabled, the tooltip
must say what would enable it.

### 2.2 Inputs

Label above, hint below, error replacing the hint. Never a placeholder as the label: it
vanishes the moment someone types, which is exactly when they need it.

Error state is a red border **plus** an icon **plus** a message. The border alone is
colour-only state.

### 2.3 Cards

`--surface`, hairline, `--radius-lg`, 12–16px padding. A card is for content that has its
own identity. A list of things that are only rows is a **table**, and making them cards
triples the vertical space to convey the same thing.

### 2.4 Menus and drop-ups

`--lift-2`, min-width 200px, items 28px tall, 8px from their trigger. The trigger stays
visibly lit while the menu is open, so it is obvious which control the panel belongs to.

Enter with `m-emerge` (scale from 0.96 + rise), 220ms. Menus grow out of their trigger; they
do not fly in from an edge.

### 2.5 Chips and badges

Pill, 10–11px, 2px vertical padding. A **chip** is removable or selectable; a **badge** is
read-only. Do not style them the same, or people click badges.

**New in v1.1 — model badges:** `FREE` (emerald pill, Gift icon, `10b981`) for any model id containing `:free` (OpenRouter convention, surfaced in OpenCode's aggregated list); `Vision` (`--accent-dim`) for multimodal; ★ favourite pinned first via `★/☆` toggle — favourites store in `localStorage:bhippi_fav_models` as `{ providerId: string[] }` and always sort atop the filtered list. Searching `/free` is a dedicated mode: it filters to `isFreeModel` only and shows a hint bar. Badges are never colour alone — they carry glyph + label.

### 2.6 Dialogs

`--lift-3`, scrim at 60% of `--bg`, max-width 560px for a decision and 880px for content.
Title, body, actions bottom-right with the primary action last (the position the eye ends on).

Escape closes. Click-outside closes only when nothing is unsaved.

### 2.7 Empty states

Three lines: what would be here, why it is not, and the one action that fills it. An empty
state with no action is a dead end, and an illustration does not count as an action.

### 2.8 Tables

Header 11px `--text-dim`, uppercase optional, sticky. Rows 32px. Numbers right-aligned and
tabular. Zebra striping only past ~15 rows; below that the hairline is enough.

### 2.9 Toasts

Bottom-right, one at a time, 4s for a confirmation and never auto-dismiss for an error. A
toast that carries the only copy of an error message is a bug.

---

## 3 · Layout

**The shell:** a fixed left rail (56px collapsed / 240px open), a content column, and an
optional right panel. Content maxes at 780px for prose — a 1600px line is unreadable
regardless of how much screen there is.

**Breakpoints:** 640 · 900 · 1200 · 1600. What is *sacrificed* at each step is a design
decision, not an accident: decide the order things drop out before writing the media query.

**Split panels** resize from a 4px handle with an 8px hit area, and remember their position.

---

## 4 · Decorative motion, and when it is right

[React Bits](https://reactbits.dev) is the sanctioned source for animated components —
165+ text animations, backgrounds, and effects, copy-paste, prop-customisable, no runtime
dependency. It is genuinely good, and it is genuinely easy to ruin an interface with.

**Reach for it on:** an empty state, a landing or marketing surface, an onboarding step, a
one-time celebration, a hero.

**Never on:** anything a user sees more than a few times a day. A shimmering gradient on a
button pressed two hundred times a session is not delight, it is a tic.

**The test:** would this still be good on the four-hundredth viewing? If the honest answer
is no, it belongs on a surface people see once.

---

## 5 · The composition rules

The difference between a screen that looks designed and one that looks assembled is not the
components — it is these ten decisions.

1. **One focal point.** Decide what the eye lands on first, and subordinate everything else.
   Two focal points is none.
2. **Three levels of hierarchy, no more.** Primary, secondary, tertiary. A fourth level is
   invisible in practice.
3. **Align to something.** Every edge shares an axis with another edge. Optical alignment
   beats mathematical alignment where they disagree — icons in particular.
4. **Repeat, don't invent.** The fourth card on a screen must look exactly like the first.
   Novelty per component is what makes an interface feel assembled.
5. **Whitespace is structure.** Grouping by spacing beats grouping by border. Reach for a
   border only when spacing genuinely cannot do it.
6. **Contrast carries meaning.** If two things look different, they must *be* different.
   Decorative variation reads as information that is not there.
7. **Limit the palette per screen.** Neutrals, one accent, and at most one semantic colour.
   A screen showing ok, warn, *and* error at once has an information architecture problem.
8. **Size by importance, not by content.** The largest element must be the most important
   one, not the one with the longest label.
9. **Every state, every time.** Empty, loading, partial, error, and full. A screen designed
   only in its full state breaks on its first day in production.
10. **Reduce until it breaks, then add one thing back.** This is the only reliable route to
    minimal that still works.

---

## 6 · The checklist

Before any surface ships:

- [ ] Every colour comes from a token; no literal hex in a component
- [ ] Body text ≥ 4.5:1, glyphs and large text ≥ 3:1
- [ ] Every interactive element has a visible `:focus-visible` ring ≥ 3:1
- [ ] No state is signalled by colour alone
- [ ] All five states designed: empty, loading, partial, error, full
- [ ] Every spacing value is on the 4px grid
- [ ] Motion is transform/opacity only, and collapses under `prefers-reduced-motion`
- [ ] Exactly one primary action
- [ ] Keyboard: tab order is sane, Escape closes, Enter submits
- [ ] Long text, long names, and zero items all render without breaking layout
- [ ] It reads correctly in light *and* dark
