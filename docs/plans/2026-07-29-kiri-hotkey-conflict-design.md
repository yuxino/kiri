# Kiri Exclusive Capture Shortcut

## Problem

Carbon's exclusive hotkey option prevents another global registration but does
not suppress a foreground application's local menu accelerator. Kiri needs to
own `⇧⌘A` at the keyboard-event level so one press cannot run two actions.

## Design

- Make `⇧⌘A` the only capture shortcut and remove the preset menu.
- Install an active session event tap at the head of the event stream.
- Consume matching key-down and key-up events so foreground applications do
  not receive the shortcut.
- Require Accessibility permission and provide a direct recovery action when
  the filter cannot be installed.
- Ignore autorepeat and re-enable a tap disabled by timeout or user input.

## Verification

- Test the fixed label, serialization, and normalized modifier model.
- Run the full core test executable and a warnings-as-errors build.
- Package and launch the app, then verify the accessibility state and active
  event tap before testing the shortcut against a foreground menu accelerator.
