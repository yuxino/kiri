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

## Recording dependency

Kiri uses FFmpeg for local recording and GIF encoding. If no usable copy is
already installed or cached, Kiri downloads the executable once when the user
first starts a recording or explicitly converts a video to GIF, then stores it
in the operating-system cache. Browsing the library never starts this
download. The dependency request contains no screenshots, recordings, filenames, library
metadata, credentials, or account identifier. Media encoding remains local.
Automatic downloads come from `ffmpeg.martin-riedl.de` on macOS and the
`GyanD/codexffmpeg` release repository on `github.com` on Windows.

## Manual update checks

Kiri does not check for application updates in the background. When the user
clicks **Check for Updates** in Settings, Kiri sends one request to the public
`api.github.com/repos/yuxino/kiri/releases/latest` endpoint. The request uses a
standard `kiri/<current-version>` user agent and contains no captures,
recordings, filenames, library metadata, OCR data, credentials, or account
identifier. Kiri does not automatically retry the request, download an update,
or install it. If a newer release exists, the user may explicitly open the
fixed Kiri Releases page in their browser.

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
