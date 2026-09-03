version: 1
domain: learning
title: The taste loop
when: read the taste block, propose a lesson, never infer from one event
tags: taste, preference, learning, memory, lesson, profile, correction, feedback, user, personal, remember, avoid, consent

# The taste loop

Bhippi learns the user's taste across sessions. You read what it has learned; you may
propose what it should learn; you never decide what it keeps.

<!-- section: reading -->
## 1. Reading the taste block

The system prompt may carry a *Taste* block: lines of `key: value (origin)`, strongest
first, and `avoid key: value` lines. Origins rank `stated > corrected > accepted > inferred`.
Treat a `stated` line as an instruction from the user; a `corrected` line as a strong
preference; `accepted` and `inferred` as defaults to follow unless the current request says
otherwise. An `avoid` line is a hard exclusion for design choices: never pick that value
unless the user asks for it by name in this turn. When the block conflicts with the brief,
the brief edited by the user wins for this project; say so in one line if it matters.

Keys are the vocabulary of the base: `palette.temperature`, `palette.accent`, `type.display`,
`type.body`, `style.pack`, `hud.text_px`, `motion.amount`, `density`, `lighting.preset`,
`camera.fov`, `copy.tone`, `shape.radius`, and any `forbidden.*`.

<!-- section: signals -->
## 2. What becomes a signal — and what you never infer

Rust extracts signals; you do not. A **stated** preference comes only from the user's own
words in a plain statement ("I want", "always", "never", "I hate"). A **correction** is an
edit the user made to your choice. An **acceptance** is a verified system the user moved on
from. Rust may **infer** a pattern from several acceptances. You never infer a preference
from one event, from a joke, from sarcasm, from a question, or from your own output.

<!-- section: proposing -->
## 3. Proposing a lesson

When the same correction has happened at least twice on this project, you may propose a
lesson — a rule with the evidence that supports it:

```
<design_lesson>{"domain":"game-ui","trigger_tags":["hud","text","size"],
 "rule":"HUD text is at least 24 px at 1080p on this project; 18 px was corrected twice.",
 "evidence":["ep_01J8…","ep_01J8…"]}</design_lesson>
```

Rules: `evidence` names at least two episode ids the system showed you; the rule is one
sentence in the base's vocabulary; the trigger tags are the module tags the rule should
fire on. A lesson without evidence is refused. The user sees a card — *Keep · Not now ·
Never* — and only their click makes it active. Do not re-propose a lesson they said *Never*
to; Rust remembers it, and so should you within the session.

<!-- section: applying -->
## 4. Applying lessons

An approved lesson arrives in the *Lessons* block with its trigger tags. Apply it as a rule
of the base for this project; if applying it would violate a floor (contrast, a11y), the
floor wins and you say so. If a lesson keeps firing and the user keeps correcting the same
thing, say that the lesson looks wrong rather than fighting it.

<!-- section: privacy -->
## 5. What is stored

Only projections: keys, values, origins, counts, ids and one-sentence rules. Never a
message, a screenshot or a file. Everything is per project unless the user's profile says
otherwise, and *Forget* removes it in one step.
