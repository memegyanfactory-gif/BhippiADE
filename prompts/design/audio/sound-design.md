version: 1
domain: audio
title: Sound design
when: the sound palette as part of the style; feedback pairing
tags: audio, sound, sfx, music, mix, bus, volume, pitch, loop, ambience, feedback, silence, ui-sound, footstep, ducking

# Sound design

Sound is half of feel and a quarter of the design. It is chosen with the brief, not after
the visuals.

<!-- section: palette -->
## 1. The sound palette

Per style pack, one family of sounds: toy and clay — soft, woody, pitched percussion; pixel
— square and triangle chip tones; neon — synth stabs and sub bass; gritty — recorded
material sounds, reverbed; painterly — acoustic instruments, wind. Every effect in the game
comes from the family; one sound from another family is the audio equivalent of a
photograph in a cartoon.

<!-- section: mix -->
## 2. Mix and buses

Buses: Master → Music, SFX, UI, Voice, Ambience. Music sits at −12 dB under gameplay, ducks
6 dB more during dialogue and on a pause menu. UI clicks at −12 dB under SFX. Nothing peaks
above −1 dB; a limiter on Master. The settings screen exposes each bus with the value in
text.

<!-- section: feedback -->
## 3. Feedback pairing

Every row of the feedback ladder (`game-ui/feedback-juice#ladder`) has a sound; the sound
carries the weight. Pickups rise in pitch on a streak (+1 semitone per pickup, reset after
1.5 s); hits are a low thump plus a short high transient; UI moves are a tick, confirms a
two-note rise, backs a one-note fall, errors a dull double. Randomise pitch ±5 % and pick
from 2–3 variants on anything that repeats.

<!-- section: ambience -->
## 4. Ambience and silence

One ambience loop per area, seamless, in the palette's family, at −18 dB; a second layer
that fades with height or depth (wind above, water below). Silence is a tool: cut the
ambience before a reveal, drop the music at a death, let the results screen breathe for a
beat before the reveal chord.

<!-- section: music -->
## 5. Music

Loops with a clean bar boundary; intensity layers (calm, tense, peak) that crossfade with
the pacing curve (`scene-3d/level-flow#pacing`); the title theme is the game's identity and
its motif returns at the win screen. Tempo matches the game's movement speed. Every track
has a licence sidecar; `unknown` blocks Release.

<!-- section: never -->
## 6. Never

A sound on every frame of movement; a music loop with an audible seam; a UI sound louder
than gameplay; an alarm that never stops; a jump scare stinger without a flash toggle
counterpart; stereo-wide UI sounds (UI is centred).
