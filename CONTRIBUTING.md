# Contributing to kiri

Thanks for helping make kiri better.

## Development setup

You need macOS 14+ and Swift 6.

```bash
swift run kiri-core-tests
swift build --product kiri -Xswiftc -warnings-as-errors
./scripts/package-app.sh
```

## Pull requests

- Keep each change focused and explain the user-facing behavior.
- Add or update core tests when changing storage, metadata, or geometry.
- Do not claim planned recording, GIF, or long-capture features are available.
- Preserve local-first behavior. Network features require an explicit user
  action and a documented privacy model.
- Avoid adding third-party dependencies unless the platform frameworks cannot
  reasonably provide the capability.

Use Issues for bugs and feature discussion. Do not include private captures,
credentials, or personal paths in reports.

