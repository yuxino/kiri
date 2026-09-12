# ADR 0033: Shared installer source

- Status: Accepted
- Date: 2026-09-12
- Supersedes ADR 0032's local artwork authoring and corner header treatment.

Maintain Windows presentation in [yuxino/desktop-installer](https://github.com/yuxino/desktop-installer).
Kiri consumes a pinned offline bundle and the same loader as the other desktop
apps. The shared repository owns templates, English / Simplified Chinese /
Japanese translations, and the dedicated Kiri half-body character master.

The welcome and finish pages use a 4x lossless true-color sidebar with Kiri's
camera-holding character. Remove the generic corner image and slogan. Thanks
and the optional GitHub/Star link remain native translated text; the link opens
only on an explicit click. Tauri continues to own installation, passive updates,
Run behavior, and uninstall. App icons and tray assets are separate from this
installer portrait.

Theme 2.1.1 uses shared dialog-unit coordinates for the native pages and HTML
preview. Presentation-only SHOW callbacks arrange existing native controls,
including Run and desktop-shortcut choices, without changing their actions.
Existing PRE/SHOW/LEAVE callbacks and the native reboot page are preserved.
Repeated image wordmarks are removed; README previews match their language.
Native controls use each language's Windows UI font. Portraits are filtered to
their actual pixel size, and text positions use the main dialog's font metrics.

Edit shared source and synchronize consumers; do not restore local generators
or edit the compressed bundle by hand. Commit the loader, bundle, and lock as
one update. Building the application requires no network request for artwork.

The source repository compiles and exercises 18 native product/language fixtures
on Windows CI, capturing the welcome, directory, and finish pages. Kiri's consumer
check verifies its Windows configuration and bundle. Physical-device scaling
and complete installed-app update acceptance remain separate tests.
