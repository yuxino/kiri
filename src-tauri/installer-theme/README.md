# yuxino installer theme v1.0.0

A small shared Windows NSIS theme: lavender accents, the approved cat illustration,
a compact header, and friendly English / Simplified Chinese / Japanese welcome
and finish copy. Each application keeps its real name and icon.

## Scope

This decorates Tauri's stock NSIS template; it does not fork the installer. Native
buttons, keyboard navigation, focus, actual progress, installation, uninstallation,
WebView2 handling, architecture selection and updater command-line arguments remain
owned by Tauri. Product names, bundle IDs, existing install directories, shortcuts,
resources, updater feeds, public keys and user data are unchanged. No app version
is bumped and no release is published by this change.

The design board was a concept, not a screenshot. This implementation uses native
wizard geometry rather than reproducing its rounded cards pixel for pixel. It
changes neither the in-app update UI nor MSI/macOS packages. Fuwa is out of scope.

## Build and shared artwork

The Windows-only Tauri configuration runs `node src-tauri/installer-theme/build.mjs`
before the existing frontend build. No npm dependency is added; use Node 22+.

The common illustration source is stored in Kiri's `src-tauri/installer-theme/artwork`.
`artwork.lock.json` fixes an immutable Kiri commit and the exact size and SHA-256 of
every data file. Kiri builds offline from its tracked copy. The other five projects
fetch only those small data files on their first Windows build, then reuse the
verified local cache. No remote JavaScript, installer hook or executable is fetched.
Missing network access fails the build, never silently falls back to unverified art.
For air-gapped builds, copy the exact locked `artwork/` directory in advance.

The source is a 32-color RGB palette plus pixel indices, Brotli-compressed and
base64-encoded in bounded pieces. The local renderer verifies the source and emits
ordinary opaque 24-bit BMPs (sidebar 164x314; header 150x57). No font files are included.
The artwork was cropped from the user-approved design supplied in this conversation.

For a direct `tauri bundle` invocation that skips before-build hooks, run the theme
build command first. Generated BMPs and fetched artwork caches are build inputs,
not application runtime assets. To change art, update the canonical source and all
six reviewed locks together; clear only this theme's old cached artwork before rebuilding.

## Checks and reuse

```sh
node src-tauri/installer-theme/build.mjs
node --test src-tauri/installer-theme/theme.test.mjs
node src-tauri/installer-theme/build.mjs --check
```

The focused Windows workflow also compiles `preview.nsi`. Its clearly named
`preview-only-setup.exe` writes no application files, registry keys or shortcuts;
it is for reviewing the native theme, not a distributable app installer.

To reuse, copy this directory, retain the lock, and merge the five NSIS theme fields
from `tauri.windows.conf.json` without replacing the receiving project's existing
configuration. Keep the original before-build command after the theme command.

Before releasing real packages, verify welcome/options/progress/finish at 100%,
150% and 200% scaling, keyboard navigation, all three languages, cancellation,
first install, signed `/UPDATE`/passive upgrade and uninstall on each supported
Windows architecture. The pure Node checks do not replace that native acceptance.
