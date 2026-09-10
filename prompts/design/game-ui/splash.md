version: 1
domain: game-ui
title: Splash screen
when: the card a game opens on, boot handover, logo, hold time, skip, legibility
tags: splash, boot, title-card, logo, brand, hold, skip, handover, main-scene, contrast, legibility, licence

# Splash screen

The splash is the first thing anyone sees of a game and the thing its author looks at least
often once it works. Both halves of that sentence are the design problem: it carries the
whole first impression, and nobody on the team will notice when it breaks.

<!-- section: handover -->
## 1. It is the boot scene, so it must hand over

A splash is not a scene the player navigates to. It **is** the project's main scene, and the
game's real first screen is what it hands over to. That inverts the usual risk: the failure
here is not an ugly card, it is a game that never starts.

Two rules follow, and both are enforced in code rather than asked for here:

- The handover target is read from the project every time, never remembered from the last
  build. When a splash is already installed, `project.godot` points at the splash, so the
  target is read back out of the generated script instead.
- A splash may never hand over to itself. A boot loop is refused at build time.

The player must be able to skip it. Any deliberate press hands over immediately. A splash
that cannot be skipped is the first thing a returning player resents, and returning players
are the ones who see it most.

<!-- section: hold -->
## 2. How long it holds

Between **three and five seconds**, and nothing else is offered. Under three it reads as a
flicker — the player registers that something happened but not what. Over five it stops
being an impression and becomes a wait, and a wait at the start of every single session is
paid for many times over.

The fades live *inside* the hold. A splash that says three seconds is on screen for three
seconds in total, not three plus a fade either side.

<!-- section: legibility -->
## 3. Legibility is the gate that matters

The lettering must clear **3:1** against the backdrop, which is WCAG AA for large text. This
is the failure mode that survives every review, because the person checking already knows
what the card says and reads it from memory rather than from the screen.

The title is set no smaller than 40px against the 720p reference height, the tagline no
smaller than 16px. The accent has to be visible against the background too — it draws the
rule under the title, and an accent nobody can see is a rule that looks like a rendering
bug.

Title, not sentence: a name up to 64 characters, a tagline up to 96. If it needs a paragraph
it is a story screen, not a splash.

<!-- section: mood -->
## 4. Reading the brief

The brief is read for a mood, and the mood carries a palette, a motion and a backdrop
together, because those three disagree badly when chosen separately. Neon wants a dark
ground and a zoom; paper wants a light ground and no movement at all. A brief that matches
no mood takes the default dark card, which is legible, rather than a random one, which is
not.

Anything the brief states outright outranks the mood it implied: an explicit `#rrggbb`, a
named motion, a named backdrop, a hold in seconds. A hold outside three to five seconds is
**ignored**, not clamped — silently rounding 30 seconds down to 5 would pretend the brief was
reasonable and hide the disagreement from the person who wrote it.

<!-- section: logo -->
## 5. The logo

A logo is optional, and when it is there it leads. It sits above the title, centred, scaled
proportionally so no logo is ever stretched, and reserved at a size that survives the
smallest window the game supports.

Every logo carries a licence, recorded in a `.meta.json` beside it, before it can ship
(INV-074). A game's boot screen is precisely the asset that ends up in a store listing and a
press kit, so an unlicensed one here is the most expensive kind to discover late.

<!-- section: export -->
## 6. Leaving the studio

A splash that only exists inside a `.tscn` is a splash the author cannot put on a store
page, a press kit or a video. It exports as an SVG any design tool opens, beside the spec
that rebuilds it and the scene and script that run it.

That export never runs the engine. An export that needed a working Godot install would fail
at exactly the moment somebody wanted the file for something outside the game.
