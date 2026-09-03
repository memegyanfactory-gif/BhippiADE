version: 1
domain: process
title: The critique
when: the ten-point rubric a screenshot is scored against
tags: critique, review, rubric, score, evidence, screenshot, visual, judge, quality, floor, repair

# The critique

The visual half of every evidence pair is judged against this rubric, by a vision model with
the rubric as its schema, and stored as an episode. A dimension is never promoted on a half
pair. Scores are 0–3 per point; the floor per surface kind is set in Rust.

<!-- section: rubric -->
## 1. The ten points

| # | Point | 3 looks like | 0 looks like |
|---|---|---|---|
| 1 | **Focal point** | the eye lands on one thing, and it is the right thing | nothing leads, or two things compete |
| 2 | **Hierarchy** | three clear levels by size, weight, colour, position | flat, or five levels |
| 3 | **Subject fit** | the palette, type and shapes could only belong to this subject | interchangeable with any product |
| 4 | **Readability** | every text passes its floor at the viewing distance; HUD text has a plate | text over a busy backdrop, tiny labels |
| 5 | **Consistency** | repeated things are one object; the fourth matches the first | every card its own design |
| 6 | **Restraint** | one accent, at most one semantic colour, one motion moment | gradients, glows, three accents |
| 7 | **States** | the visible state is complete: no clipped text, no empty slot, no placeholder | lorem, a spinner in a void, a cropped label |
| 8 | **Composition (3D/2D)** | silhouettes separate; foreground / midground / background read; scale cues present | flat value, no depth, floating props |
| 9 | **Light and material (3D)** | one key, a warm/cool contrast, roughness discipline, shadows where they explain form | flat lighting, plastic sheen everywhere, black shadows |
| 10 | **Fit to intent** | matches the plan card's brief and the user's words | drifted from the brief |

<!-- section: visual-judge -->
## 2. How the judge is asked

The schema: `{ scores: [10 × 0..3], worst: { point, why, fix }, subject_detail: string | null,
drift: string | null }`. The judge sees the screenshot, the plan (or brief) and the surface
kind — never the conversation. `fix` is one concrete change in the vocabulary of the base
(a section id when one applies), so a sub-floor score becomes a repair round with a target,
not a vibe.

<!-- section: scoring -->
## 3. Floors and what happens under them

| Surface | Floor (sum of 30) | Points that must each be ≥ 2 |
|---|---|---|
| studio chrome | 24 | 4, 5, 7 |
| web page | 22 | 1, 4, 7 |
| game UI (HUD, menu) | 22 | 4, 7 |
| 3D scene | 20 | 8, 9 |
| 2D scene | 20 | 4, 8 |

Under the floor: one repair round carrying `worst.fix`; a second failure pauses with a
decision card, never a loop. Above the floor with a `drift` note: the build continues and
the drift is shown to the user on the plan card.

<!-- section: self-critique -->
## 4. Self-critique before claiming done

Before you say a surface is finished, run the ten points on your own result and name the
weakest one. If it is under 2, fix it first. "It looks fine" is not a score.
