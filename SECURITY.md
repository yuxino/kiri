# Security policy

Please do not open a public Issue for a vulnerability involving capture
contents, local file access, permissions, or unintended data disclosure.

Report security issues privately through GitHub Security Advisories for this
repository. Include the affected revision, reproduction steps, and expected
impact. Remove personal screen contents, tokens, and credentials from logs or
sample files.

kiri is local-first and does not upload captures automatically. A change that
transfers capture contents or other user data must make the destination and
user action explicit.

Optional remote OCR follows that rule per image. The app prepares only the
selected region locally, discloses the configured provider origin, model, and
image size, and sends it only after the user activates a visible Send or Retry
action. Return uses local OCR for that image. Failed requests are never retried,
rerouted, or uploaded through a fallback provider automatically.

Remote OCR API keys are write-only inputs stored in macOS Keychain or Windows
Credential Manager. They must not appear in profile JSON, IPC responses, logs,
screenshots, crash reports, fixtures, or environment-variable fallbacks.
