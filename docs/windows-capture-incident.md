# Windows capture incident

Status: fix candidate; Windows native retest required.

This document tracks the Windows screenshot failure on PR #3 without treating
CI or a packaged installer as native acceptance. Keep the pull request open;
do not merge or publish from this incident workflow.

## Reported behavior

Two Windows behaviors must remain distinct:

1. The earlier candidate opened capture but did not render the frozen desktop
   or capture controls. That failure was reproduced and traced to private
   `kiri` resource URLs being formed and parsed differently by Windows
   WebView2.
2. After the resource URL repair, the user reported that completing a
   screenshot made Kiri disappear and that launching it again appeared to do
   nothing. This second behavior does not yet have a Windows log or completed
   native retest, so its exact process state is not considered proven.

## Host-side findings

The second symptom had three concrete gaps in the application:

- `confirm_capture` is a synchronous Tauri command. It directly closed the
  overlay WebView that owned the still-pending IPC request before returning a
  response. Destroying WebView2 inside its command callback creates a lifecycle
  race at the exact point the screenshot UI disappears.
- Windows release builds use the GUI subsystem and have no console. The prior
  `env_logger` target therefore left no durable startup, capture, window, panic,
  or single-instance trail after the window vanished.
- A second launch is intentionally forwarded to the resident process by
  `tauri-plugin-single-instance`. The callback only tried to show an existing
  `library` window and ignored every error. If that window had been destroyed,
  the new process exited after forwarding and no visible window was recreated.

The single-instance plugin uses a Windows mutex owned by the process. A mutex
cannot remain owned after a real process death. Therefore a launch that is
silently forwarded points to a still-running or unresponsive first process,
not a permanently orphaned single-instance lock.

## Fix candidate

- Overlay destruction is dispatched from an async worker and reaches the main
  event loop only after the synchronous confirmation callback has returned its
  IPC response.
- Windows writes an immediate log with startup rotation to
  `%LOCALAPPDATA%\io.yuxino.kiri\logs\kiri.log`. At startup, a file larger than
  4 MiB becomes `kiri.previous.log`; one previous file is retained. Rust panics
  include their location, thread, payload, and backtrace.
- Startup, display freeze, frozen-resource delivery, confirmation milestones,
  window destruction, completion preview, process exit, and second-instance
  reopen are recorded without capture pixels, OCR text, credentials, or launch
  arguments.
- A second-instance request now restores and focuses the library window, or
  recreates it when the original window no longer exists. Failures are logged.

## Native retest evidence required

Do not mark the incident fixed until one exact x64 installer from the PR head
has all of the following evidence on Windows:

1. Before screenshot: visible Kiri library and a harmless source window.
2. Capture: frozen desktop, mode selector, region selection, and confirmation.
3. After screenshot: saved item visible in Kiri and image data present on the
   clipboard.
4. Relaunch: starting Kiri again brings the library to the foreground whether
   the resident process is visible or hidden.
5. Log: the same run contains `display frozen`, `frozen capture served`,
   `validated`, `session consumed`, `library import complete`, `completion
   preview presented`, `completion flow returned`, and `deferred overlay
   close`, with no `[panic]` entry.

For the native test, read the final log lines with:

```powershell
Get-Content "$env:LOCALAPPDATA\io.yuxino.kiri\logs\kiri.log" -Tail 200
```

Record the installer name, SHA-256, PR head, CI run, Windows version and
architecture, before/after screenshots, copied-image result, process state,
and the redacted log excerpt. A green CI run proves compilation and packaging;
it does not replace these interactions.
