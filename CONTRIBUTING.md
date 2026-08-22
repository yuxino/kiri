# Contributing to kiri

Thanks for helping make kiri better.

## Development setup

Use Rust 1.88+, Node.js 20.19+ (or 22.12+), and pnpm. macOS development
requires macOS 14+; Windows-specific behavior is also built in CI.

```bash
pnpm install --frozen-lockfile
pnpm test:release-tools
pnpm build
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
git diff --check
```

`src-tauri` is the only Rust workspace. Do not add a parallel root Cargo
workspace or a second Tauri application. Capture tests must use injected data
or an isolated harness; normal development and production builds always capture
the real desktop and use the real local library.

For macOS capture, recording, focus, or permission work, package with a stable
signing identity through `./scripts/package-app.sh`. Do not reset privacy
permissions or use the user's capture library as test data.

## Pull requests

- Keep each change focused and explain the user-facing behavior.
- Add or update core tests when changing storage, metadata, or geometry.
- Keep feature claims aligned with current source, `AGENTS.md`, accepted ADRs,
  and `docs/architecture.md`.
- Preserve local-first behavior. Network features require an explicit user
  action and a documented privacy model.
- Remove unused dependencies and avoid adding new ones without a concrete
  platform or product need.

Use Issues for bugs and feature discussion. Do not include private captures,
credentials, or personal paths in reports.
