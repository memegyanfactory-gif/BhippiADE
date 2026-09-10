version: 1
domain: foundations
title: Motion
when: named durations and easings, transform-only, reduced motion
tags: motion, animation, transition, easing, duration, tween, stagger, reduced-motion, reveal, hover, decorative

# Motion

Every animation answers "what changed, and where did it come from".

<!-- section: durations -->
## 1. Durations are named for the kind of change

| Token | ms | Change |
|---|---|---|
| `--t-instant` | 90 | press / release |
| `--t-quick` | 140 | hover, focus, small flips |
| `--t-move` | 220 | a panel travels |
| `--t-enter` | 300 | something arrives |
| `--t-settle` | 420 | arrives and comes to rest |
| `--t-ambient` | 1600 | "this is alive" loop |

In Godot the same table drives `Tween` durations (seconds ÷ 1000); the names travel.

<!-- section: easings -->
## 2. Easings by change

`ease-out` for arrivals, `ease-in` for departures, `ease-in-out` for anything that moves and
stops, a spring **only** for a confirmation, linear only for a continuous loop. Godot:
`Tween.EASE_OUT` + `TRANS_CUBIC` for arrivals, `TRANS_BACK` for the one spring, `TRANS_LINEAR`
for loops.

<!-- section: transform-only -->
## 3. Transform and opacity only

Animating `width`, `height`, `top` or `margin` forces layout on every frame. Move with
`transform`, fade with `opacity`; in Godot, tween `position`, `scale`, `rotation` and
`modulate`, never a container's `custom_minimum_size` every frame.

<!-- section: stagger -->
## 4. Stagger

Siblings stagger by 34 ms, capped at about twelve — past that the last row waits half a second
for a reason nobody perceives. Menus grow out of their trigger (scale from 0.96 plus a rise);
they do not fly in from an edge.

<!-- section: reduced-motion -->
## 5. Reduced motion

Every animation collapses under `prefers-reduced-motion` to an instant state change or a
plain fade. In a game the same lives in Settings › Accessibility (screen shake, flashes,
camera bob), on by default to respect, never buried.

<!-- section: at-rest -->
## 6. Show the page at rest

Everything meant to be read is visible once the page has loaded, without scrolling to trigger
it — that first still frame is what a thumbnail, a shared link and a skimming reader get. A
section may animate in, but from a visible resting state, never parked at `opacity: 0`
waiting on an observer. A hero is sized to what it holds, not to the viewport.

<!-- section: decorative -->
## 7. Decorative motion, and when it is right

Reach for it on an empty state, a landing or marketing surface, an onboarding step, a
one-time celebration, a title screen, a hero. Never on anything seen more than a few times a
day. The test: would this still be good on the four-hundredth viewing? If the honest answer
is no, it belongs on a surface people see once. One orchestrated moment lands harder than
scattered micro-interactions; scattered effects are one of the surest signs of a generated
page.
