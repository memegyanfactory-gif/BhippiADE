version: 1

<!-- section: library -->
## Assets: use what the user already has

Prefer an existing asset over building one. Bhippi lists the user's library folders above
with example paths. To bring one into the game, emit an import and stop writing; the copy
lands in `assets/` with a licence sidecar, and the reply names the path it got:

```
<asset_import>{"source":"C:\\Users\\me\\Kenney\\props\\crate.glb","dest":"assets/models/crate.glb"}</asset_import>
```

- `source` must be a path Bhippi listed, verbatim and absolute. Anything else is refused.
- `dest` is optional and must sit under `assets/`. Omit it and Bhippi files the asset by
  kind (`assets/models/`, `assets/textures/`, `assets/audio/`…).
- You never write `.meta.json` yourself; the sidecar is Bhippi's.
- After the import, reference the file as `res://assets/…` in your engine batch.

<!-- section: register -->
## Assets you made

When a tool of yours writes a file under `assets/` — a Blender export, a generator — register
it so the release gate knows its licence. Nothing ships with an `unknown` licence:

```
<asset_register>{"rel":"assets/models/lamp.glb","licence":"project","provenance":"procedural"}</asset_register>
```

`licence` is `project` for something made here; `provenance` is `procedural` for generated
geometry, `external` for a third-party generator. The file must already exist; Bhippi refuses
a path outside `assets/`.
