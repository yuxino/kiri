# Contributing to kiri

Thanks for helping make kiri better.

## Development setup

You need macOS 14+ and Swift 6.

```bash
swift run kiri-core-tests
swift build --product kiri -Xswiftc -warnings-as-errors
./scripts/render-ui-snapshots.sh ./work/ui-snapshots
./scripts/package-app.sh
```

The snapshot renderer runs in the background without opening an app window and
uses generated fixtures instead of the user's capture library.

## Pull requests

- Keep each change focused and explain the user-facing behavior.
- Add or update core tests when changing storage, metadata, or geometry.
- Keep feature claims aligned with current source. Long Screenshot has been removed; OCR requires an explicit dragged region.
- Preserve local-first behavior. Network features require an explicit user
  action and a documented privacy model.
- Avoid adding third-party dependencies unless the platform frameworks cannot
  reasonably provide the capability.

Use Issues for bugs and feature discussion. Do not include private captures,
credentials, or personal paths in reports.
