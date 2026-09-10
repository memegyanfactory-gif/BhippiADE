version: 1
domain: scene-3d
title: Model selection
when: placing or replacing any mesh: request, score, fit, place
tags: model, mesh, asset, glb, gltf, kenney, cc0, library, blender, procedural, csg, scale, footprint, bounds, pivot, poly, licence, placement, prop, character, select, score, fit, replace, generate

# Model selection

Every mesh that enters a scene goes through this process. The model never names a file path
and never invents a scale; it describes what is needed, Rust scores what exists, and the
winner is placed with its computed bounds. When nothing fits, a blockout is placed on purpose
— never the nearest wrong asset.

<!-- section: request -->
## 1. The request

Say what the thing *is* and what it is *for*, in the style's vocabulary, with its role and
its fit:

```
{"kind":"asset.request","asset":{"kind":"mesh","tags":["lamp post","street","iron"],
 "style":"low-poly-toy","role":"prop",
 "fit":{"max_height_m":3.0,"footprint_m":[0.5,0.5],"poly_budget":800,"pivot":"base","up":"y"},
 "place":{"scene":"scenes/main.tscn","parent":"Level/Props","position":[12,0,-4],"orient":"path"}}}
```

- `tags`: the noun first, then material and setting words; the style pack's asset tags are
  added by Rust from the brief.
- `role`: `hero` (the focal thing; the budget is generous), `prop` (dressing), `set`
  (walls, floors, large structure), `ground`, `fx`. The role sets the default poly budget
  and how strict the style fit is.
- `fit`: the bounds the blockout established (`scene-3d/layout-metrics#blockout`). A request
  without `fit` is refused — a mesh without a size is not a request.
- `place`: where it goes and how it orients (`path`, `camera`, `random_yaw`, a fixed yaw).

Ask for a **character** the same way with `role: "hero"` and `fit` from the player metrics
table; ask for a **set piece** with `role: "set"` and the blockout's exact bounds.

<!-- section: candidates -->
## 2. Candidates, from every tier

Rust gathers up to twelve candidates, in this order, all before scoring:

1. **Procedural presets** — CSG and primitive builds (crate, barrel, tree, rock, fence,
   pillar, platform, vehicle-from-primitives). Zero tokens, always licensed, always the right
   scale; the right answer for `set` and `ground` more often than not.
2. **Bundled CC0 library** — the hash-pinned pack, by tag index, with style tags.
3. **The user's library folders** — anything registered in Assets, indexed by file name and
   sidecar tags; a fit beats a download.
4. **Generation** — Blender over MCP (procedural, licence `project`) or an opt-in
   text-to-3D provider (metered, sidecar carries the provider's terms). Only when the
   first three produce no candidate over the floor, and only for `hero` and `prop` roles.

A candidate with an `unknown` licence is listed and never wins.

<!-- section: scoring -->
## 3. Scoring

Each candidate gets a score in Rust from six terms; the table is stored in the journal row
so "why this one" has an answer.

| Term | Formula | Weight |
|---|---|---|
| style fit | Jaccard of candidate style tags and the brief's style tags; a tag on the pack's **never-mix** list scores −1 | 4 |
| tag fit | Jaccard of candidate tags and request tags, noun match counted double | 3 |
| scale fit | `1 − clamp(|ln(candidate_height / fit.max_height_m)|, 0, 1)` | 3 |
| poly fit | 1 within budget · 0.5 within 2× · 0 beyond | 2 |
| provenance | project 1.0 · cc0 0.9 · user library 0.8 · generator 0.6 | 1 |
| licence | gate: `unknown` → cannot win | — |

Floor: a normalised score of 0.55. Ties break toward the earlier tier. For a `hero` the
style-fit weight doubles; for `set` and `ground` the scale-fit weight doubles.

<!-- section: fit-check -->
## 4. Fit check on the winner

Before placement Rust reads the winner's bounds (glTF accessor min/max — no mesh decode —
or the `.tscn` AABB for CSG) and checks:

- **units**: metres; a model whose height is 100× or 0.01× the request is in centimetres
  or inches — auto-scale by the power of ten, once, and record it.
- **scale window**: after unit correction the height must land within 0.5×–2× of
  `max_height_m`; inside the window it is scaled to fit exactly; outside, the candidate is
  rejected and the next one is tried.
- **pivot**: the origin must be at the base for props and characters (`base`), at the
  centre for `fx` and floating things; a wrong pivot is corrected by wrapping in a `Node3D`
  with the offset, never by editing the file.
- **up axis**: +Y; a Z-up import is rotated −90° on X in the wrapper.
- **footprint**: the XZ extent must fit the requested footprint within 1.5×, or the prop
  will not sit in its cluster.

<!-- section: placement -->
## 5. Placement

- **Ground snap**: raycast down from `position + 5 m` on the world layer; place the base on
  the hit; if nothing is hit, refuse and say so (a floating prop is never silently placed).
- **Grid snap**: 0.5 m for props, 1 m for `set`, none for `fx`.
- **Orient**: `path` aligns −Z to the nearest `Path3D` tangent; `camera` faces the default
  camera; `random_yaw` picks a yaw from the request's seed in 15° steps; a fixed yaw is used
  as given.
- **Clearance**: ≥ 0.3 m from sibling props' bounds, ≥ 1.5 m from any interactable's clear
  floor; a collision moves the prop along the placement's tangent up to 1 m, else refuses.
- **Collider**: a static body from the simplified shape (`scene-3d/layout-metrics#collision`)
  unless the role is `fx`.
- The batch instances the scene (`instance_scene`), sets the wrapper's transform, and
  records the sidecar and the score table in the journal — one row, one Undo.

<!-- section: fallback -->
## 6. When nothing fits

Place a **CSG blockout** with the requested bounds in the neutral material, named for the
request (`LampPost_todo`), in group `bhippi_todo`, and say so in the reply: what was asked,
what the best candidate was and why it failed the floor. The level keeps its gameplay
dimensions; dressing can come later. Never substitute the nearest wrong thing — a modern
office chair in a medieval tavern scores below the floor by design.

<!-- section: replace -->
## 7. Replacing a model

"Replace the crates with barrels" is a request with the existing node's bounds as `fit`, the
same `place`, and the new tags; the old instance is removed in the same batch. A replacement
that changes a gameplay dimension (a taller wall) is refused with the dimension named.

<!-- section: characters -->
## 8. Characters and animated meshes

A character candidate must carry a skeleton and the clips the archetype's player preset
expects (idle, walk, run, jump; attack for action games), or a retarget path to a library
rig; a mesh without them is dressing, not a character. Height from the player metrics table;
the collider is a capsule from the bounds; the `AnimationTree` is wired by the preset, never
hand-built.

<!-- section: checklist -->
## 9. Before calling a placement done

- the request carried role and fit; the fit came from the blockout
- the winner's score table is in the journal; no `unknown` licence
- units corrected once; height inside the window; pivot at base; +Y up
- grounded by raycast; snapped; oriented; cleared from siblings
- collider matches the shape; the scene diffs as one node
