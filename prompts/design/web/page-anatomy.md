version: 1
domain: web
title: Page anatomy
when: landing, docs, tool, app shell, game export shell and credits
tags: web, page, website, landing, marketing, hero, docs, documentation, dashboard, tool, app, shell, export, credits, store, itch, section, cta, proof

# Page anatomy

The treatment is decided in `process/design-plan#calibrate`. This is what each kind of page
is made of.

<!-- section: landing -->
## 1. Landing and marketing pages

- **Hero as thesis**: the most characteristic thing in the subject's world — a headline that
  states the offer in one sentence, an image or live moment that could belong to nothing
  else, and one call to action. Sized to what it holds, not `100vh`.
- **Proof before features**: testimonials, logos, numbers, a screenshot — from the user's
  material, or visibly marked placeholders. Never fabricated.
- **Benefits that answer doubts**, not a feature list. Each section answers a question a
  visitor actually has.
- **One primary action**, repeated down the page in the same words. Not three competing
  buttons.
- Copy is the product: specific, grounded in what the user said, in their voice. Where a fact
  is missing, `[PLACEHOLDER]`, never invention.
- Check it at a phone width before delivering: wrapping headlines, squashed grids, text too
  small.

<!-- section: docs -->
## 2. Documentation and reading pages

One column at the measure; a sticky, quiet table of contents on wide screens; headings that
are names, not sentences; code in its own scrolling box; a search that works from the
keyboard. Numbered steps only where order matters. The type does the design.

<!-- section: tool -->
## 3. Tools and dashboards

Scanned and operated, not read. Surface the summary before the detail; encode state in form
as well as number — a pill, a chip, a severity stripe — so what needs attention reads at a
glance. Semantic colour is separate from the accent. What is interactive looks interactive.
Filters in one row above the content. Charts follow `web/charts`. Dense grid, 13 px base,
tabular numerals.

<!-- section: app-shell -->
## 4. App shells

`foundations/space-layout#shell`. The rail carries navigation, the top bar carries context
(where am I, what is running), the content column carries the work, the right panel carries
inspection. Never two scrollbars in one column. Every panel has its five states.

<!-- section: export-shell -->
## 5. The game's web export shell

The page that hosts the exported game:

- The canvas fills the viewport at the game's aspect ratio, letterboxed on a ground taken
  from the game's brief (`bg`), never black by default and never stretched.
- Above the fold before load: the game's title in its display face, a one-line description,
  a **Play** button that starts audio context and fullscreen on a user gesture, and the
  loading progress as a bar with a percentage in text.
- Below: controls in a small table (keyboard and gamepad), the credits link, the version.
- No dependency on the studio: no IPC, no localhost, no analytics call.
- Works with the keyboard; the canvas gets focus on Play; Escape leaves fullscreen.

<!-- section: credits -->
## 6. The credits page

Generated from licence sidecars; the design is a reading page: the game's title, the
studio's name, then sections by kind (models, textures, audio, fonts, code), each row
*name — author — licence — link*, in the body face at the measure, no cards. Attribution
text is verbatim from the sidecar. The page reads in both themes.

<!-- section: store -->
## 7. Store and listing pages (itch, Steam, a site)

A landing page whose proof is the game itself: a trailer or a real gameplay capture at the
top, three to five real screenshots at the game's aspect ratio, the description in the
game's voice, the platform list, the price or `[PRICE]`, and one button. The palette is the
game's brief, so the page and the game are one thing.
