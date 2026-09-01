# Contributing to Bhippi

Thank you for helping improve Bhippi. This repository uses specification-led development: behavior, enforcement, tests, and documentation move together.

## Before making a change

Read these files in order:

1. [`docs/07-AGENT-GUIDE.md`](docs/07-AGENT-GUIDE.md)
2. [`docs/PROGRESS.md`](docs/PROGRESS.md)
3. [`docs/08-BUILD-ORDER.md`](docs/08-BUILD-ORDER.md)
4. The relevant section of [`docs/02-MODULE-CONTRACTS.md`](docs/02-MODULE-CONTRACTS.md)
5. The named rules in [`docs/06-INVARIANTS.md`](docs/06-INVARIANTS.md)

When documents disagree, follow the authority order defined in the agent guide. Structural deviations require an ADR before implementation.

## Development rules

- Keep changes scoped to one ticket or one coherent fix.
- Do not add dependencies, screens, configuration axes, or crate edges without documented authority.
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

- The ticket or problem being solved.
- The invariant IDs touched and their enforcement points.
- The tests that prove acceptance.
- Any remaining limitations or environment-dependent checks.
- Screenshots for visible UI changes when practical.

Small, reviewable commits are preferred. Do not mix cleanup with unrelated behavior changes.
