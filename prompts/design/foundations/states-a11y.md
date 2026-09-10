version: 1
domain: foundations
title: States and accessibility
when: the floors and the five states, as a gate list
tags: accessibility, a11y, contrast, focus, keyboard, state, empty, loading, error, colour-blind, reduced-motion, screen-reader, gamepad, subtitle, floor, gate

# States and accessibility

These are gates. A surface that fails one is not done, however it looks.

<!-- section: floors -->
## 1. The floors

- Body text ≥ 4.5:1; large text and UI glyphs ≥ 3:1; focus ring ≥ 3:1 against both the
  component and the page.
- Every interactive element is keyboard reachable in a sane order; Escape closes, Enter
  submits; a visible `:focus-visible` ring on everything focusable.
- No state signalled by colour alone — a glyph, a label, a position or a shape rides with it.
- Motion collapses under `prefers-reduced-motion`; a game exposes shake, flash and camera bob
  toggles.
- Hit targets ≥ 44 px on touch surfaces; ≥ 24 px with pointer.
- Text can be enlarged 200 % without loss; a game offers a UI scale slider.
- Every image that carries meaning has an alt; every icon-only control has a label.
- Every colour comes from a token; no literal hex in a component.
- Every spacing value is on the grid.

<!-- section: five-states -->
## 2. The five states, designed every time

**Empty · loading · partial · error · full.** A screen designed only in its full state breaks
on its first day. Loading keeps layout stable (a skeleton the shape of the content, not a
spinner in a void, and a loading button keeps its width). Partial shows what arrived and says
what has not. Error shows the message *and* the last good content. Empty follows
`foundations/copy#empty-states`.

<!-- section: long-content -->
## 3. Hostile content

Long text, long names, zero items, a thousand items, a 4:3 screen, a 21:9 screen, a
200 % font scale — all render without breaking layout. Names truncate with an ellipsis and a
full title on hover; numbers never truncate.

<!-- section: games -->
## 4. Games in particular

- Subtitles on by default, sized like HUD text, with a scrim; speaker names when more than
  one character talks.
- Remappable controls; gamepad and keyboard both complete; a focus neighbour set on every
  menu control (`game-ui/godot-control#focus`).
- Colour-blind safe gameplay signals: pickup, hazard and goal differ in shape and brightness,
  not only hue; offer a colour-blind mode that swaps the palette, not the shapes.
- Photosensitivity: no full-screen flashes above three per second; a flash toggle.
- Difficulty and assist options are accessibility, not cheating: an aim assist, a hold-to-
  toggle option, a game-speed slider.

<!-- section: checklist -->
## 5. The checklist before a surface ships

- [ ] Every colour from a token; body ≥ 4.5:1, glyphs ≥ 3:1
- [ ] Visible focus ring ≥ 3:1 on every interactive element
- [ ] No state by colour alone
- [ ] Empty, loading, partial, error, full — all five designed
- [ ] Every spacing on the grid; every font size on the scale
- [ ] Motion transform/opacity only and collapses under reduced motion
- [ ] Exactly one primary action
- [ ] Keyboard and gamepad: order sane, Escape/Back closes, Enter/A confirms
- [ ] Long text, long names, zero items render without breaking
- [ ] Reads correctly in light *and* dark (or commits to one, explicitly)
