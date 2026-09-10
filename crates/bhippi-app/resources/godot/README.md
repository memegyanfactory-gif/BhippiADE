# The engine that ships with Bhippi

This directory holds the pinned Godot build that `tauri build` bundles into the installer
(ADR-0047). The binary itself is **not** in git — it is ~120 MB — so this file is here to keep
the directory, and with it the `bundle.resources` glob, valid in a fresh checkout.

Populate it with:

    node scripts/fetch-godot.mjs

That downloads the pinned release from the official GitHub release, checks it against the
recorded SHA-256, and unpacks it here. It is idempotent, and `tauri build` runs it for you via
`beforeBuildCommand`, so a packaged build can never be missing the engine.

`node scripts/fetch-godot.mjs --check` answers the same question without downloading anything,
which is what CI should ask before it packages.

A `cargo run` from a checkout that has never fetched works fine: detection falls through to a
Godot found on `PATH` or in a standard install folder, exactly as it did before ADR-0047.
