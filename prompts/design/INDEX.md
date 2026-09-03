version: 1
format: bhippi-design-kb@1

# Bhippi design base — the map

This is the only part of the base that is always in context. One line per module: the id
you ask for, and when to read it. Ask with `<design_query>{"kind":"section","id":"module#section"}</design_query>`
or `{"kind":"search","q":"…"}`. The pack Rust hands you each turn already holds the sections
most likely to matter; ask when you need one more.

Order is load-bearing: it is the tie-break order for selection.

## foundations — every surface
- `foundations/judgements` — the six calls every other rule serves; read first when unsure
- `foundations/color` — ramps, the one accent, semantic colour, floors, chosen neutrals
- `foundations/type` — pairing by role, the scale, measure, weights, numerals
- `foundations/space-layout` — grid, proximity, alignment, gap over margin, hierarchy
- `foundations/shape-elevation` — radius by size, concentric rule, elevation by role
- `foundations/motion` — named durations and easings, transform-only, reduced motion
- `foundations/copy` — words as design material: names, controls, errors, empty states
- `foundations/states-a11y` — the floors and the five states, as a gate list
- `foundations/anti-slop` — the generated-looking defaults and what to do instead
- `foundations/icons-imagery` — icons on a grid, never emoji; illustration and photo rules

## process — before and after the work
- `process/design-plan` — calibrate treatment, write the token plan, honour what exists
- `process/critique` — the ten-point rubric a screenshot is scored against
- `process/handoff` — the spec a design needs before it becomes a batch or a page

## web — pages, sites, the export shell
- `web/page-anatomy` — landing, docs, tool, app shell, game export shell and credits
- `web/fonts` — pairing method, vetted pairings by mood, loading, fallback stacks
- `web/themes-responsive` — three-state theming, breakpoints, the sacrifice order
- `web/dynamic` — interactive pages: state, loading, forms, tables, libraries
- `web/charts` — form before colour, the four colour jobs, the six checks, marks

## game-ui — HUD, menus, Godot Control
- `game-ui/hud` — what earns screen, safe area, readable at distance, bars, minimap
- `game-ui/menus-flow` — title to play in two inputs, pause, settings, results, focus
- `game-ui/godot-control` — anchors, containers, Theme as tokens, focus, scaling
- `game-ui/feedback-juice` — hit-stop, shake budget, tweens, particles as punctuation

## scene-3d — the world
- `scene-3d/composition` — one focal point, silhouettes, layers, reading order
- `scene-3d/layout-metrics` — the metric grid, player metrics, derived gap and door numbers
- `scene-3d/level-flow` — pacing, gating, guidance, sight lines, safe zones
- `scene-3d/lighting-environment` — key light, sky, temperature contrast, environment settings
- `scene-3d/materials-palette` — palette per style, value ranges, roughness discipline
- `scene-3d/camera` — FOV by perspective, follow, framing, dead zones
- `scene-3d/model-selection` — placing or replacing any mesh: request, score, fit, place

## scene-2d — sprites and tiles
- `scene-2d/sprites-tiles` — pixel grid, filtering, seams, parallax, silhouettes

## art-direction — the brief and the styles
- `art-direction/brief` — the brief every later decision is checked against
- `art-direction/styles` — twelve coherent style packs and what they never mix with

## audio
- `audio/sound-design` — the sound palette as part of the style; feedback pairing

## learning — the taste loop
- `learning/taste-loop` — read the taste block, propose a lesson, never infer from one event
