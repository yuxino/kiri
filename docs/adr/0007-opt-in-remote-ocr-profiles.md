# ADR 0007: Opt-in remote OCR profiles

Date: 2026-08-22

## Status

Accepted

## Context

Kiri's OCR implementation uses the operating system's local text-recognition
framework. That remains the safest default, but it cannot cover every language,
document, or model-quality requirement. A single hard-coded cloud vendor would
also make a poor fit for an open-source application: users need to choose their
own endpoint, model, account, and data boundary.

Remote OCR changes Kiri's privacy model because pixels from a selected region
can leave the device. Saving a remote profile must therefore never make capture
or OCR upload silently.

## Decision

Kiri supports multiple named OCR profiles in addition to its built-in local
engine.

- Local OCR is always present, is the initial active engine, needs no account,
  and keeps the existing automatic recognition flow.
- The first remote adapter uses the OpenAI-compatible Chat Completions image
  contract. Alibaba Cloud Model Studio and OpenAI are convenience presets;
  custom compatible base URLs and model names remain editable.
- Creating or editing a profile does not activate it. Profiles may be selected
  explicitly, and deleting the active profile returns the app to local OCR.
- Selecting a remote profile does not authorize an upload. For every selected
  image, Kiri first prepares the crop locally and shows the profile name,
  destination origin, model, pixel dimensions, and byte size. Only the visible
  Send or Retry action performs the network request. Return chooses local OCR
  for that image; failures never retry automatically, switch providers, or fall
  back silently.
- API keys are write-only application inputs stored in macOS Keychain or
  Windows Credential Manager. Profile JSON contains only non-secret metadata;
  IPC responses expose `hasApiKey`, never the credential itself. There is no
  environment-variable or plaintext-file fallback.
- Provider requests originate in Rust, not the WebView. Remote URLs require
  HTTPS, except explicit loopback development endpoints. Redirects are disabled
  and request, response, and timeout limits are enforced.
- Prepared crops live only in bounded, expiring memory and are owned by the
  active overlay. Cancelling or destroying that overlay discards them.

## Consequences

- Kiri remains fully useful without cloud configuration or network access.
- The library window gains a Settings destination for language and OCR profile
  management; it does not gain a separate settings process or window.
- The overlay needs a distinct remote-consent state instead of routing its
  existing automatic local OCR command through the active provider.
- English, Simplified Chinese, and the existing Japanese runtime dictionary
  must describe the local/remote boundary consistently.
- User-facing documentation must replace absolute "never uploads" wording with
  the narrower guarantee that captures remain local unless the user explicitly
  sends the current OCR selection to a configured provider.
- Additional provider-native protocols can be introduced as new adapters
  without changing stored profiles or weakening the per-image consent rule.
