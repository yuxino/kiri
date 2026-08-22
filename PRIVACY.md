# Privacy

Kiri is local-first. Screenshots, recordings, annotations, OCR results, and the
capture library stay on the device unless the user exports them or explicitly
sends a selected OCR region to a configured remote provider. Kiri has no
account system, analytics, advertising, or telemetry.

## Local OCR

Local OCR is enabled by default and uses macOS Vision or Windows.Media.Ocr. It
does not require an API key or network connection, and image pixels do not
leave the device.

## Optional remote OCR

Users may save multiple Alibaba Cloud, OpenAI, or image-capable OpenAI Chat
Completions-compatible OCR profiles. Merely creating or selecting a profile
does not send data. Before every remote request, Kiri shows the profile,
destination origin, model, and selected image dimensions and size. Only
activating a visible Send or Retry action sends that selected PNG together with
a short text-recognition instruction and the configured model name.

Return uses local OCR for that image. A failed remote request is never retried,
routed to another provider, or uploaded through a fallback automatically. Kiri
does not send the full capture, the capture library, other screen regions, or
previous OCR results as part of the request.

The chosen provider processes the request under its own terms and retention
policy. Custom profiles send data to the origin shown in Kiri's confirmation
card; users should only configure endpoints they trust.

## Credentials and configuration

API keys are entered inside Kiri and stored in macOS Keychain or Windows
Credential Manager. Kiri's profile JSON stores only non-secret metadata such
as a profile name, base URL, and model. API keys are not returned to the UI,
written to logs or plaintext configuration, or loaded from environment
variables.

Deleting a profile removes its associated system credential. Users can return
to fully local operation at any time by selecting Local OCR and deleting remote
profiles.
