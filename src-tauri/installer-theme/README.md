# Kiri installer

Presentation source: [yuxino/desktop-installer](https://github.com/yuxino/desktop-installer).
Pinned version: **2.1.1**. This directory is generated; edit the shared repository.

Build offline from the application root:

```sh
node src-tauri/installer-theme/build.mjs
node --test src-tauri/installer-theme/theme.node.mjs
node src-tauri/installer-theme/build.mjs --check
```

The Windows config uses native Tauri/NSIS installation and update behavior, a lossless
4x full-color half-body sidebar, and English / Simplified Chinese / Japanese dialogs.
The optional GitHub link opens only when clicked on Finish.

Update all consumers from the shared repository:

```sh
npm run sync -- --root <parent-of-application-repositories>
```

Commit the resulting bundle and lock together. Old releases keep their pinned artwork.
No network, Swift, image service, or package install is needed to unpack this bundle.
