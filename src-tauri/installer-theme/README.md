# yuxino installer theme v1.0.0

Shared lavender/cat NSIS branding for Kiri, Mimi, Satori, Viva, Tick and WNACG.
Fuwa is excluded. The artwork comes from the user-approved design; each app keeps
its real name and icon. Welcome/finish copy supports English, Chinese and Japanese.

This decorates Tauri's stock NSIS installer, not a replacement installer or a
pixel-identical rendering of the concept board. Native controls, progress, install
policy, shortcuts, resources, user data, updater keys/feeds and uninstall remain
unchanged. In-app update UI, MSI, macOS, versions and releases are out of scope.

## Build and verify

Windows beforeBuildCommand generates the artwork before the original app build.
Node 22+ is sufficient; there are no extra npm dependencies. The common artwork is
held in Kiri and pinned by immutable commit, exact size and SHA-256 in the lock.
Kiri has a tracked copy; other projects fetch only the locked data on first build
and reuse the verified cache. No remote JavaScript or executable is downloaded.
For offline builds, pre-copy the exact locked artwork directory. Preserve its bytes
with the included .gitattributes. A corrupt or missing source fails closed.

```sh
node src-tauri/installer-theme/build.mjs
node --test src-tauri/installer-theme/theme.node.mjs
node src-tauri/installer-theme/build.mjs --check
```

The .node.mjs suffix keeps Node-only checks out of browser/Vitest discovery; all
seven checks still run in the dedicated Windows theme workflow. The renderer emits
opaque 24-bit BMPs: sidebar 164x314 and header 150x57. No font files are included.
Run the generator explicitly before direct tauri bundle commands that skip hooks.

## Reuse and acceptance

Copy this folder and merge the NSIS appearance fields into the receiving project's
Windows overlay without replacing its existing configuration/build command.
Update common artwork and all six reviewed locks together; remove only this theme's
obsolete cached artwork when deliberately updating the theme version.

preview.nsi produces a clearly named, harmless UI preview with no app installation,
registry edits or shortcuts. It does not replace real package acceptance. Before
releasing, verify first install, cancel, signed passive update, uninstall, keyboard
navigation, all languages and 100/150/200% DPI on each supported Windows architecture.
