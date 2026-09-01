# ADR-0007: Build on the stable toolchain; raise effective MSRV to 1.85
Date: 2026-08-26 · Status: accepted · Supersedes: ADR-0004 (toolchain half only)
Date basis: session log 2026-08-26 (Codex row) already flagged local stable ≠ pinned 1.79.

## Context

ADR-0004 pinned the repo to Rust 1.79 so `specta =2.0.0-rc.22` (edition 2021) could serve
INV-032 code generation. That held for the S0 crates. Adding the provider layer's HTTP stack
(`reqwest` → `rustls` → `chacha20`/`quinn`) resolves to crates whose manifests require
`edition2024`, which cargo ≥ 1.85 parses. Downgrading the entire crypto/TLS chain to mid-2024
versions would freeze security patches behind an ever-growing pin list — exactly the failure
mode ADR-0004 tried to avoid, now worse.

## Decision

1. `rust-toolchain.toml` moves from `1.79.0` to `stable` (today 1.98). CI installs stable.
2. Workspace `rust-version` becomes `1.85` — the honest floor imposed by `edition2024`
   dependencies. Our own code keeps its existing discipline (`unsafe_code = forbid`,
   clippy `unwrap_used`/`expect_used` denied).
3. The INV-032 generator moves as a locked set: `tauri-specta =2.0.0-rc.24` +
   `specta =2.0.0-rc.24` + `specta-typescript 0.0.11`. ADR-0004's `specta =rc.22` pin is
   superseded — its pairing partner (`tauri-specta rc.21` → `specta-typescript ^0.29`) no
   longer resolves on the registry, and the edition-2024 concern it guarded against is moot
   under this ADR. Regeneration: `cargo run -p bhippi-app --bin export-bindings`.
4. Dependency floors elsewhere are unpinned again; Cargo.lock carries reproducibility.

## Consequences

- Easier: modern TLS/crypto stacks and Tauri 2.x current releases compile without pinning.
- Harder: contributors must run Rust ≥ 1.85 (stable). The MSRV table in spec §4 loses meaning
  beyond "our own crates are 1.85-clean"; a future `cargo msrv` check may restore a real floor.
- Docs changed: `PROGRESS.md` risk table notes the bump; `Cargo.toml` updated in this change.

## Alternatives rejected

- Pin ~10 transitives back to 2024 editions: fragile, silent security drift, repeated forever.
- Stay on 1.79 and drop reqwest (hand-rolled TLS): unacceptable effort and risk for zero gain.
