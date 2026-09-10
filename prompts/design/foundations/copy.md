version: 1
domain: foundations
title: Copy
when: words as design material: names, controls, errors, empty states
tags: copy, microcopy, label, button, error, empty, title, naming, tone, voice, cta, placeholder, tooltip

# Copy

Words are design material, not decoration. Write from the user's side of the screen.

<!-- section: user-side-naming -->
## 1. Name things by what people recognise

A person manages *notifications*, not *webhook config*; a player picks *Continue*, not
*Load last checkpoint slot*. Name the thing by its effect, in the user's vocabulary, and by the
subject's terms of art where the subject has them (a racing game says *laps* and *grid*, not
*rounds* and *start positions*).

<!-- section: controls -->
## 2. Controls say exactly what happens

Active voice, verb first: **Publish**, then a toast that says *Published*. Never *OK* / *Yes*
for a consequential action — the button carries the consequence: *Delete level*, *Keep
editing*. A disabled control's tooltip says what would enable it. One primary action per
screen, and it is named for the screen's job.

<!-- section: errors -->
## 3. Errors explain and fix

What went wrong, in one sentence; how to fix it, in the next; no apology, no vagueness, no
error code as the only content. *"Godot 4.2 found; this project needs 4.3 or newer. Install
4.7.1 from Settings › Engine."* A toast never carries the only copy of an error.

<!-- section: empty-states -->
## 4. Empty states

Three lines: what would be here, why it is not, and the one action that fills it. An empty
state with no action is a dead end, and an illustration does not count as an action.

<!-- section: titles -->
## 5. Titles are names

A page, a game, a level, a menu is named like a product, not captioned: a short noun phrase
specific to the subject, without an appended explainer after a dash or colon. *Harbour Run*,
not *Harbour Run — a cosy fishing racer*. The explanation goes in the subtitle or the
description, never in the title.

<!-- section: real-content -->
## 6. Real content, never lorem

Build with real content throughout. Where a real fact is missing (a price, a date, a studio
name), put a visibly marked placeholder — `[YOUR PRICE]` — for the user to fill; never
fabricate one. Never *Welcome to our website*, never interchangeable filler that could
describe any product. Specific beats clever.

<!-- section: tone -->
## 7. Tone

Match the subject's register and hold it: a survival game's death screen is terse; a kids'
puzzle game's is warm. Humour is a choice made once in the brief, not sprinkled. No emoji in
UI copy unless the brand uses them.
