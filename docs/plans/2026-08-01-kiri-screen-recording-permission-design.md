# Kiri Screen Recording Permission Design

## Goal

Stop repeated Screen Recording prompts while keeping the first capture request
clear and recoverable. Kiri must never modify privacy settings automatically.

## Permission flow

A session-scoped permission gate checks the system preflight result before every
capture. If access is already active, capture continues immediately. If access
is inactive and Kiri has not requested it during this launch, the gate calls the
system request API once and caches the outcome for the rest of the process.

- A granted request becomes `restartRequired` because ScreenCaptureKit access
  takes effect after Kiri quits and reopens.
- A declined request becomes `settingsRequired`.
- Further capture attempts return the cached outcome without invoking another
  system request.
- A later successful preflight overrides and clears the cached outcome.

The cache is intentionally not persisted. Each new launch reads the operating
system as the source of truth and may make one fresh request if access is still
missing.

## Recovery UI

Capture permission errors use the existing library and menu-bar notices with
one explicit recovery action:

- `restartRequired`: explain that access was granted and offer **Quit Kiri**.
- `settingsRequired`: explain that access is off and offer **Open Settings**.

Kiri never opens System Settings or quits unless the user presses the action.
Closing the notice does not trigger anything.

## Testing

The gate is pure KiriCore logic with injected preflight and request closures.
Tests cover authorized access, a granted first request, a declined first
request, cached repeat attempts, and a later successful preflight. App builds
and packaging are verified without launching Kiri.
