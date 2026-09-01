# ADR-0004: Hold MSRV 1.79 with explicit dependency pins
Date: 2026-08-26 · Status: accepted
Amends: `01-ARCHITECTURE.md` §12 (workspace manifest) · Implements `00-SPEC-v1.0.md` §4 (MSRV 1.79)

## Context

The spec pins Rust 1.79 and `rust-toolchain.toml` pins it for every build. The lockfile
generated during S0 was resolved by a much newer cargo and drew in crates the pinned
toolchain cannot build:

- `specta 2.0.0-rc.23+` and `tauri-specta 2.0.0-rc.24+` ship as edition 2024, which cargo
  1.79 refuses to parse at all;
- `time 0.3.55`, `indexmap 2.14`, and friends declare a rust-version far above 1.79;
- `idna 1.x` (pulled by `url 2.5.1+`, in turn by `sqlx-core`) uses `core::error`, stable only
  from 1.81, while declaring a much lower rust-version — its own MSRV claim is wrong.

Left alone, the workspace only built on the machine's newest stable, which makes the MSRV
claim in the spec, the CI matrix in BHP-009, and the toolchain file all fiction.

## Decision

Keep MSRV 1.79 and make the dependency graph obey it.

1. `.cargo/config.toml` sets `[resolver] incompatible-rust-versions = "fallback"`, so any
   future re-resolution prefers versions that still build on 1.79. Cargo 1.79 itself warns
   that the key is unknown; that warning is expected and is the cost of the guarantee.
2. `specta` is pinned to `=2.0.0-rc.22` — the newest edition-2021 release, and the exact
   version `tauri-specta 2.0.0-rc.21` requires, so INV-032 code generation stays available.
3. `Cargo.lock` additionally pins `url 2.5.0` (hence `idna 0.5`), because no `idna 1.x`
   release compiles on 1.79 regardless of what its manifest claims.

`Cargo.lock` is committed and authoritative. Do not run a bare `cargo update`; add or move
one dependency at a time and re-run the gates on the pinned toolchain.

## Consequences

`cargo fmt`, `cargo clippy -D warnings`, and `cargo test --workspace` all pass on 1.79
locally, so CI can assert the same thing on three platforms and mean it. The cost is that a
few dependencies sit behind their latest release, and every new dependency has to be checked
against 1.79 before it lands — `tauri 2.x` (MSRV 1.77.2) and `tauri-specta 2.0.0-rc.21` were
both verified compatible before BHP-008.

If a dependency the spec requires ever becomes unavailable at 1.79 — the likely candidates
are the S5 search and embedding crates — that is a spec-level decision and needs its own ADR
raising the MSRV, not a silent local bump.

## Alternatives considered

- Raising the MSRV now was rejected because nothing in S0–S4 requires it, and the spec's
  1.79 floor is a deliberate compatibility promise, not an accident.
- Building on the machine's newest stable and ignoring the toolchain file was rejected: it
  makes the MSRV unverified everywhere and pushes the failure into the CI matrix.
- Vendoring or patching `idna` was rejected as far more maintenance than pinning `url`.
