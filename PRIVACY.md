# Privacy

Kiri is local-first. Screenshots, recordings, annotations, OCR results, and the
capture library stay on the device unless the user exports them or explicitly
sends a selected OCR region to a configured remote provider. Kiri has no
account system, analytics, advertising, or telemetry.

## Library storage

Kiri keeps one managed library in the operating-system application-data
location by default. Settings can copy it to another local directory or
external disk and switch to that copy; the previous copy is retained. The
selected location contains the library index, captures, and editable screenshot
projects.

If a completed recording cannot be imported into the active library, Kiri
keeps it in a local recovery area until a later import succeeds.

## Editable screenshot projects

For annotations created by current Kiri builds, the local library keeps the
shareable flattened screenshot together with a clean source image and a small
annotation document. The clean source can still contain pixels hidden by
mosaic, text backgrounds, or shapes in the flattened result. These project
files stay in Kiri's active library, are never uploaded by the editing feature,
and remain recoverable when the screenshot is moved to Trash.
They are removed when that screenshot is permanently deleted. Older flattened
screenshots do not contain reconstructable annotation data; starting a new edit
uses the current flattened image as its source.

## Local media processing

Kiri records, merges paused segments, generates video thumbnails, and converts
MP4 files to GIF with operating-system media frameworks. macOS uses
AVFoundation and ImageIO; Windows uses Media Foundation and Windows imaging
components. Kiri does not download or execute FFmpeg or another third-party
media binary. Screenshots, recordings, filenames, library metadata,
credentials, and media bytes remain on the device during these operations.

## Manual update checks

Kiri does not check for application updates in the background. When the user
clicks **Check for Updates** in Settings, Kiri requests the fixed public
`github.com/yuxino/kiri/releases/latest/download/latest.json` manifest over
HTTPS. If a newer release exists, Kiri displays its version and release notes.
It does not download anything until the user clicks **Download Update**, and it
does not install anything until the user clicks **Install Update**. The official
Tauri updater verifies the downloaded archive against Kiri's embedded public
key before the interface enables installation. macOS restarts only after a
further explicit click; on Windows, the installer closes Kiri and completes the
update. A fixed Releases-page link is shown only as recovery after a failure.

Update requests contain the installed platform and version but no captures,
recordings, filenames, library metadata, OCR data, credentials, telemetry, or
account identifier.

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
