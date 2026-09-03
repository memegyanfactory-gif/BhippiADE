version: 1
domain: game-ui
title: Feedback and juice
when: hit-stop, shake budget, tweens, particles as punctuation
tags: juice, feedback, hit-stop, screen-shake, squash, stretch, particle, tween, punch, impact, pickup, flash, vignette, sound, four-hundredth

# Feedback and juice

Feedback tells the player that an action landed. Juice is feedback with taste. Both are
subject to the four-hundredth-viewing test: the pickup that delights at minute one must not
grate at hour four.

<!-- section: ladder -->
## 1. The feedback ladder — spend by consequence

| Event | Feedback |
|---|---|
| a routine action (footstep, jump) | sound, a small squash |
| a pickup | sound, a scale tick on the counter, the item's particle burst, a short pitch rise on streaks |
| a hit given | hit-stop 40–60 ms, a flash on the target, a small shake |
| a hit taken | hit-stop 60–80 ms, a vignette pulse (140 ms), a stronger shake, the health bar ghost drain |
| a kill / a goal / a checkpoint | the one big moment: slow-mo 200 ms, a particle bloom, a chord |
| a level end | the results screen's orchestrated reveal |

Nothing above the row it belongs to. A routine action with a big moment's feedback is the
most common form of over-juice.

<!-- section: shake -->
## 2. Screen shake

A trauma value 0–1 that decays (`trauma -= 2.0 * delta`), shake amplitude `trauma²`, offset
from noise not random, rotation ≤ 2°, applied to the camera's `h_offset`/`v_offset` or a
parent `Node3D`, never to the HUD layer. Ceiling: 8 px at 1080p for the biggest hit, 2 px
for the smallest. A toggle and a slider in Accessibility. Never a shake on a UI action.

<!-- section: squash-stretch -->
## 3. Squash and stretch

Jump: scale `(0.9, 1.1)` on takeoff, `(1.1, 0.9)` on landing, back to `1` over `--t-quick`
with `ease-out`. Keep volume roughly constant (one axis up, the other down). Apply to the
mesh or sprite child, never the collision shape. Buttons squash 0.96 on press and spring
back — the one place the spring easing is right.

<!-- section: particles -->
## 4. Particles as punctuation

A burst is a full stop: one-shot `GPUParticles2D/3D`, ≤ 24 particles, ≤ 0.6 s, in the
event's semantic colour (pickup colour for pickups, hazard colour for hits), with size and
alpha ramping to zero. Continuous emitters are for the world (dust, embers, rain), at low
density, never on the player unless the mechanic is the trail. A particle that hides what
the player needs to see is a bug.

<!-- section: tweens -->
## 5. Tweens

`create_tween()` per event, `set_parallel()` for the property set, durations from the motion
table, `TRANS_CUBIC` + `EASE_OUT` for arrivals, `TRANS_BACK` only for the one confirmation.
Kill the previous tween on the same node before starting another. Tween `scale`, `position`,
`modulate` and `rotation`; never a container's size every frame.

<!-- section: flash -->
## 6. Flashes and hit-stop

A hit flash is the target's material `albedo` (or sprite `modulate`) to white for 60–80 ms,
never a full-screen flash. Full-screen flashes stay under three per second and have a
toggle. Hit-stop is `Engine.time_scale = 0.05` for 40–80 ms on a real timer (not a scaled
one), UI unaffected.

<!-- section: sound-pairing -->
## 7. Sound pairs with everything

Every row of the ladder has a sound, and the sound carries more of the feel than the visual:
a pickup with a rising pitch on a streak, a hit with a low thump plus a short high crack,
UI with a click at `−12 dB` under the music. Randomise pitch ±5 % on repeated sounds. See
`audio/sound-design`.

<!-- section: test -->
## 8. The test

Play for ten minutes. Anything you noticed more than twice is too loud; anything you never
noticed is doing its job or is missing — check the ladder to know which.
