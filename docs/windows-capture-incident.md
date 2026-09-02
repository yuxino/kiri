# Windows capture incident

Status: PR #3 merged; v1.4.8 follow-up candidate; Windows native retest required.

This document tracks the Windows screenshot failures around PR #3 without
treating CI or a packaged installer as native acceptance. The pull request is
merged; do not publish the v1.4.8 follow-up without the native evidence below.

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

## PR #3 fix

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

## v1.4.8 follow-up: capture startup stall

A later Windows run recorded the shortcut registration and every key press, but
the first `start_capture: beginning capture flow` was never followed by either
`display frozen` or a capture error. The user pressed the shortcut again before
the first native freeze returned, and subsequent requests entered the same
startup boundary. Relaunch requests continued reaching the resident process,
confirming that the process was alive rather than blocked by an orphaned
single-instance mutex.

PR #3 did not guard this startup interval or bound the complete `xcap` call; its
three-second frame wait does not cover monitor setup, D3D/WinRT calls, cleanup,
PNG encoding, or window enumeration. The v1.4.8 follow-up therefore:

- permits only one native display freeze at a time while preserving the
  overlay's re-entry for an already-created capture session;
- coalesces shortcut and tray requests before dispatch, retaining one owned
  permit through execution and timeout recovery;
- runs the complete Windows startup on a dedicated thread, leaving the Tauri
  event loop free while the desktop frame and Overlay WebView2 controller are
  created;
- runs the Windows freeze in one worker with an eight-second native-frame
  deadline and a non-destructive post-processing slow-work warning;
- refuses to accumulate replacement workers if the original OS call remains
  active after the timeout; and
- logs monitor enumeration, first-frame request/arrival, window enumeration,
  and worker completion without recording window titles or capture pixels.

The first v1.4.8 candidate applied the eight-second deadline to the complete
worker. Native testing on Windows ARM64 in UTM showed that WGC returned a frame
in less than two seconds, but PNG encoding plus window enumeration could move
the final result just past eight seconds. The worker completed successfully
milliseconds after its caller had already discarded the result. A later live
test exposed a second issue: `xcap::Window::all()` could query a Kiri-owned
window while Kiri's UI thread was synchronously awaiting the worker, creating a
cross-thread wait and leaving the app unresponsive. The corrected candidate
therefore keeps the eight-second deadline only until the native frame arrives,
uses fast lossless PNG encoding, and collects hit-test bounds directly through
`EnumWindows` and DWM. It rejects the current process before any potentially
blocking window metadata lookup. Post-processing records a slow-work warning
after 30 seconds but keeps waiting for a successful result instead of creating
the same deadline race at a later threshold. Timing logs separate monitor
enumeration, WGC, display metadata, PNG encoding, window enumeration, and
display-index resolution.

Two further native runs separated the remaining failures. In one run the WGC
call returned only after 15.4 seconds, well beyond Kiri's eight-second caller
deadline; the worker could not be cancelled and its late result was discarded.
In another run the complete worker finished in 979 milliseconds, including a
four-millisecond direct window collection, but the process then stopped after
`display frozen` and Windows recorded `AppHangB1`. Phase logging localized that
stall to creation of the second WebView2 controller. Kiri had been running the
whole capture start inside `run_on_main_thread`, so both a slow native frame and
WebView2's synchronous controller initialization occupied a nested Tauri event
callback.

The final candidate uses xcap's GDI/BitBlt backend for the frozen still while
retaining Windows Graphics Capture for recording. It also invokes
`start_capture` from a dedicated Windows thread. Tauri then posts dynamic
window creation to its normal event-loop boundary instead of constructing the
second WebView2 controller from inside a main-thread callback. This keeps the
resident UI responsive during the eight-second deadline and avoids the nested
WebView2/COM initialization deadlock. The protocol, session publication,
WebView build, placement, configuration, and focus phases each have explicit
milestone logs.

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
   queued`, `completion preview presented`, and `completion flow returned`,
   with no `[panic]` entry.
6. Startup recovery: press the shortcut repeatedly during one capture start.
   Exactly one background start and one native worker may run. A missing GDI
   frame must restore the originating window with a visible timeout within
   eight seconds while Kiri remains responsive; once a frame arrives, slower
   post-processing must continue to its result and must not be discarded at
   either the old eight-second boundary or the slow-work warning. Queued
   key-repeat events must not replay after the result. A later deliberate retry
   must not create another worker while a timed-out native worker is still
   active.

For the native test, read the final log lines with:

```powershell
Get-Content "$env:LOCALAPPDATA\io.yuxino.kiri\logs\kiri.log" -Tail 200
```

Record the installer name, SHA-256, PR head, CI run, Windows version and
architecture, before/after screenshots, copied-image result, process state,
and the redacted log excerpt. A green CI run proves compilation and packaging;
it does not replace these interactions.
