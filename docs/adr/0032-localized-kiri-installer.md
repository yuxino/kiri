# ADR 0032: A localized, product-specific Windows installer

- Status: Accepted
- Date: 2026-09-12

The shared installer used a generic girl/cat illustration, English raster
slogans, and low-resolution header artwork. The welcome and finish copy felt
forced, and its internationalization did not cover product-specific messages.

Use Kiri's existing app character unchanged, white backgrounds, dark neutral
text and native NSIS page hierarchy. Render the sidebar and header at four times
their native sizes from the original icon, with lossless full-color output.
Artwork contains only the proper name Kiri. Keep explanations, captions,
buttons, completion guidance and errors in localizable text controls.

Ship English, Simplified Chinese and Japanese. Use the system UI language with
English fallback. Translate the welcome/finish captions and all Tauri installer
messages, including maintenance, WebView2, running-app handling and uninstall.
Let the NSIS language tables choose an appropriate dialog font. Do not display
an extra language dialog during passive updates or fork the stock installer.

Automated checks cover message-key and placeholder parity, configuration,
source-icon identity, lossless asset integrity and 4x dimensions. Per the user's
chosen workflow, Windows appearance and upgrade acceptance are tested by the
user after source changes; local checks do not claim that device acceptance.
