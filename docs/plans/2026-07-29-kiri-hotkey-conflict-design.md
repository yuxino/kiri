# Kiri Hotkey Conflict Fix

## Problem

Kiri's default `⇧⌘A` shortcut conflicts with Codex's built-in
"Archive chat" command. Carbon's exclusive hotkey option prevents another
global hotkey registration, but it does not suppress a foreground
application's own menu accelerator. Both actions can therefore run from one
key press.

## Design

- Make `⌥⌘2` the default capture shortcut.
- Remove `⇧⌘A` from the selectable presets so users cannot reintroduce the
  known conflict from Kiri's menu.
- Migrate both historical default identifiers, `shiftCommand2` and
  `shiftCommandA`, to `optionCommand2`.
- Keep `⌃⇧2` as the alternate preset.
- Update the user-facing documentation in all maintained languages.

## Verification

- Test the preset labels, serialization, modifier model, and both legacy
  migrations.
- Run the full core test executable and a warnings-as-errors build.
- Package and launch the app, then confirm the stored preset and running
  executable use the new build.
