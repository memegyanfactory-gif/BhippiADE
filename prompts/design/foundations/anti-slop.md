version: 1
domain: foundations
title: Avoiding the generated look
when: the generated-looking defaults and what to do instead
tags: slop, generic, template, default, cliche, gradient, emoji, inter, rounded, centered, ai-look, distinctive, structure, numbering, hero

# Avoiding the generated look

Generated design clusters around a few looks. Where the user pins a direction, follow it
exactly — their words always win, including when they ask for one of these. Where nothing is
specified, do not spend that freedom on a default.

<!-- section: the-looks -->
## 1. The looks to recognise and avoid

- Warm cream (`#F4F1EA`) with a serif display and a terracotta accent.
- Near-black with a lone acid-green or vermilion pop.
- Broadsheet hairline rules with dense columns, for a subject that is not a newspaper.
- A purple-to-blue gradient hero on white; aggressive gradient backgrounds anywhere.
- Inter, Roboto, Arial, Space Grotesk or Fraunces as the "safe" face.
- Emoji as section markers or icons.
- Everything centred; `rounded-lg` on everything; an accent bar or rail on every card.
- Three cards in a row with an icon, a heading and two lines each, repeated down the page.
- Numbered markers (01 / 02 / 03) on content that is not a sequence.
- Sparkle, glow and floating-orb decoration; "data slop" — stats and numbers that decide
  nothing.
- In games: the default Godot theme; a grey box HUD; a title screen that is the game's name
  in a system font over the first level; neon-on-black for a subject that is not neon.

<!-- section: structure-as-information -->
## 2. Structure is information

Numbering, eyebrows, dividers and labels encode something true about the content or they are
dropped. Number a list only when order carries information. An eyebrow says the section's
kind only when the kinds differ. A divider separates things that are actually separate.

<!-- section: what-to-do-instead -->
## 3. What to do instead

- Take the palette, the type and the shape language from the subject
  (`foundations/judgements#ground-in-subject`, `art-direction/brief`).
- Spend boldness in one place — the display face, the hero, one motion moment — and keep
  everything around it quiet.
- Match complexity to the vision: a minimal direction needs precision in spacing, type and
  detail; a maximal one needs elaborate execution. Elegance is executing the chosen vision
  well.
- Vary across projects: never converge on the same choices twice for different subjects.

<!-- section: build-cleanly -->
## 4. Build cleanly

Watch for overlapping elements, cascade collisions and silent font fallbacks. Close every
element, quote every attribute, give focus a visible state. Structure selectors so a
type-based rule and an element-based rule do not fight over the same padding. For generative
or decorative graphics, use Canvas or WebGL rather than hand-authored long SVG paths. A page
that needs a library loads a pinned build from an allowed CDN before the script that uses it
(`web/dynamic#libraries`); most pages need none.
