# Kiri multi-profile OCR implementation plan

> - Date: 2026-08-22
> - Source branch: `main` at v1.3.0
> - Decision: `docs/adr/0007-opt-in-remote-ocr-profiles.md`

## Outcome

Keep operating-system OCR as Kiri's zero-configuration default while allowing
users to save and switch among multiple Alibaba Cloud, OpenAI, or image-capable
OpenAI Chat Completions-compatible OCR profiles. Remote recognition must be an
explicit action for each selected image.

## Data and credential boundary

Persist `schemaVersion`, `activeEngine`, and an array of profiles containing
`id`, optimistic `revision`, display name, provider preset, protocol, base URL,
and model. Store that metadata atomically under the app configuration directory.
Store each API key separately in the operating-system credential store under a
stable service name and profile ID. API keys are never serialized, logged, sent
back over IPC, or read from configuration files or environment variables.

Invalid or missing configuration fails closed to local OCR. A profile cannot be
activated without a saved credential. Updating a key and its metadata is
compensated if persistence fails; deleting an active profile selects local OCR.

## OCR request flow

1. The overlay sends only the selected rectangle to Rust.
2. Rust validates the active capture session and calling overlay, crops the
   frozen display in memory, and stores a five-minute pending request owned by
   that overlay.
3. For the local engine, the overlay immediately asks the local Vision or
   Windows OCR implementation to recognize the pending crop.
4. For a remote engine, Rust returns a non-secret disclosure snapshot. The
   overlay displays provider, origin, model, dimensions, and size without
   starting network activity.
5. Return or "Use Local This Time" invokes local OCR. Only an explicit "Send"
   or "Retry" action invokes the remote command.
6. The remote command revalidates owner, profile ID, profile revision, and
   credential before sending. A changed profile requires a new disclosure.
7. Success clears the pending crop. Failure retains it until retry, local use,
   cancellation, overlay teardown, or expiry; there is no automatic retry or
   fallback.

## Remote adapter limits

- OpenAI-compatible `/chat/completions` image request with an editable model.
- HTTPS only; HTTP is accepted only for exact loopback hosts and bypasses system
  proxies.
- Redirects disabled, 10-second connect timeout, 45-second overall timeout.
- 20 MiB PNG limit and 1 MiB response limit.
- Sanitized status and transport errors that do not include credentials, image
  data, response bodies, or complete query strings.

## UI work

- Add a Settings icon to the library header and render settings in the same
  window. Keep Library and Trash as a separate two-way collection switch.
- Move the language control into a General section.
- Show Local OCR as the permanent first engine card, followed by a multiple
  profile list with active, credential, model, endpoint, edit, and delete states.
- Use a focused add/edit sheet. Presets seed defaults without locking fields;
  editing never displays a stored key, and an empty key input preserves it.
- Add an overlay consent card that is visually related to the existing OCR HUD.
  Its primary keyboard action is local OCR; remote Send is pointer-activated.
- Reuse Kiri design tokens, add visible keyboard focus, and keep all strings in
  English, Simplified Chinese, and Japanese dictionaries with identical keys.

## Verification

- Rust tests: local default, multi-profile round trip, validation, revision
  changes, active-profile deletion, metadata secret exclusion, fake secret-store
  compensation, pending ownership/expiry/cancellation, remote request and
  response bounds, and error redaction. Tests never access the real credential
  store, network, or capture library.
- Frontend: TypeScript build, dictionary key/placeholder parity, and manual
  inspection at the minimum library size and over light/dark capture content.
- Repository checks: full Rust tests/check, frontend production build, and
  `git diff --check`.
