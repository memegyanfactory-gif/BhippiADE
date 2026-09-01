# Contributing to Bhippi

Thank you for helping improve Bhippi. Keep changes focused, testable, and honest about what is implemented versus planned.

## Before making a change

Review the repository map in [README.md](README.md), then inspect the owning crate and its tests. Preserve existing ownership boundaries and state any structural tradeoff clearly in the pull request.

## Development rules

- Keep changes scoped to one coherent feature or fix.
- Explain new dependencies, screens, configuration axes, or crate edges in the pull request.
- Keep business logic in Rust; TypeScript renders state and handles viewport presentation.
- Do not hand-edit `ui/src/lib/ipc.ts`; regenerate it from Rust.
- Do not use `unwrap()` or `expect()` outside tests.
- Keep SQL inside `bhippi-db`.
- Keep prompts in versioned files under `prompts/`.
- Never add a bypass for robots, paywalls, licences, permissions, or release gates.
- Never commit credentials, local databases, logs, generated builds, or workspace-agent state.

## Verification

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm test --prefix ui
npm run build --prefix ui
```

If the IPC command surface changed:

```bash
cargo run -p bhippi-app --bin export-bindings
git diff --exit-code -- ui/src/lib/ipc.ts
```

UI changes must preserve loading, empty, error, and populated states; keyboard access; visible focus; reduced-motion behavior; and non-color-only meaning.

## Pull requests

In the description, include:

- The problem being solved and why the chosen boundary is appropriate.
- The enforcement points for safety, validation, persistence, and permissions.
- The tests that prove acceptance.
- Any remaining limitations or environment-dependent checks.
- Screenshots for visible UI changes when practical.

Small, reviewable commits are preferred. Do not mix cleanup with unrelated behavior changes.
