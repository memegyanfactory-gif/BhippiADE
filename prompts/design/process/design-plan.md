version: 1
domain: process
title: The design plan
when: calibrate treatment, write the token plan, honour what exists
tags: plan, process, treatment, calibrate, tokens, palette, typeface, layout, existing, precedence, brief, before-code

# The design plan

Before code, before a batch: read the request, calibrate the treatment, and write a plan the
size of a paragraph. Then build from the plan and nothing else.

<!-- section: calibrate -->
## 1. Read the request first — calibrate treatment, not whether to design

A doc, a settings page, a debug panel deserve the same craft as a landing page; what changes
is the **treatment**. Utilitarian: a plan, a memo, a tool, a HUD — polished hierarchy,
considered spacing, a proper palette, no flourish. Editorial: a landing page, a title
screen, a store page, a game people will share — an opinionated identity, one real aesthetic
risk. When unsure, a well-composed utilitarian page is never wrong; an over-designed identity
sometimes is. Most pages do not need a gigantic hero.

<!-- section: honour-existing -->
## 2. Honour what is already there

Precedence: the user's own words → the project's existing system (tokens, a theme file, the
art-direction brief, existing screens, an existing Godot `Theme`) → your choices. Inside an
existing product, match it pixel for pixel before extending it: lift exact values from the
real stylesheet or `.tres`, never rounded to a grid. Say in one line what you matched. The
base fills gaps; it never overrides a system the user already has.

<!-- section: plan -->
## 3. The plan — a compact token system

Write it before any code, in this shape:

- **Subject and job**: one concrete subject, its audience, the surface's single job.
- **Colour**: 4–6 named values (`bg`, `surface`, `text`, `accent`, and one or two more), with
  the neutral's temperature and the accent's hue stated. For a game, the same five plus the
  gameplay semantics (pickup, hazard, goal).
- **Type**: 2+ roles — a characterful display face used with restraint, a complementary body
  face, a utility face if data or code appears — with fallback stacks.
- **Layout**: one or two sentences. The shell, the measure, the focal point.
- **Motion**: the one moment that moves, if any.
- **The subject's detail**: the one thing only this subject would have.

For a 3D scene the plan is the art-direction brief (`art-direction/brief`) plus the level's
focal point and camera; for a HUD it is the HUD budget (`game-ui/hud#budget`).

<!-- section: review -->
## 4. Review the plan against the subject

If any line of the plan reads like the default for any similar page, revise that line and
say what changed and why. Only then write code. A plan that survives this review is the
spec; the code follows it exactly.

<!-- section: look-once -->
## 5. Write, look once, deliver

Build from the plan. Look at the result once — one screenshot, one visual observation — and
make one pass of edits for what it shows. Then deliver. Do not build a loop of screenshots
around your own work; the live result is the review surface, and further polish is the
user's to ask for. If the user reports something visibly broken, fix that and deliver once
more.
