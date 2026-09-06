"""One-time bounded fix for capture overlays triggered over another app's macOS full-screen Space."""
from pathlib import Path
import subprocess
R=Path.cwd()
def edit(path,old,new):
 p=R/path;s=p.read_text();assert s.count(old)==1,(path,s.count(old));p.write_text(s.replace(old,new))
edit('src-tauri/src/platform/macos.rs','''    if policy.screen_saver_level {
        ns_window.setLevel(NSScreenSaverWindowLevel);
    }
}''','''    if policy.screen_saver_level {
        ns_window.setLevel(NSScreenSaverWindowLevel);
    }
    // A shortcut may be pressed while another application owns a native
    // full-screen Space. Changing collectionBehavior makes this window
    // eligible for that Space, but a window that was created before the
    // policy change can remain behind the full-screen owner until it is
    // explicitly reordered. orderFrontRegardless is intentionally
    // non-activating: it keeps the video/app in its full-screen Space instead
    // of switching the user back to Kiri's ordinary Space.
    if policy.full_screen_auxiliary {
        ns_window.orderFrontRegardless();
    }
}''')
edit('src-tauri/src/commands.rs','''    log::info!("create_overlay_window: focusing window label={label}");
    if let Err(error) = window.set_focus() {
        let _ = window.close();
        return Err(error.into());
    }
    log::info!("create_overlay_window: window focused label={label}");''','''    #[cfg(windows)]
    {
        log::info!("create_overlay_window: focusing window label={label}");
        if let Err(error) = window.set_focus() {
            let _ = window.close();
            return Err(error.into());
        }
        log::info!("create_overlay_window: window focused label={label}");
    }
    #[cfg(target_os = "macos")]
    log::info!("create_overlay_window: ordered over active full-screen Space without activation label={label}");''')
edit('src-tauri/src/commands.rs','''    // Make the app active so the overlay webview receives keyboard input
    // (Esc/Return/tool keys) while the user interacts with it.
    platform::activate_self();''','''    // Windows needs explicit activation so the overlay receives keyboard
    // input. On macOS, activating Kiri here can leave another application's
    // native full-screen Space. The AppKit transient policy orders the overlay
    // above that Space without activation; clicking the overlay then gives it
    // normal interaction focus without a Space switch.
    #[cfg(windows)]
    platform::activate_self();''')
edit('src-tauri/src/platform/macos.rs','''        assert!(behavior.contains(NSWindowCollectionBehavior::FullScreenAuxiliary));
        assert!(!behavior.contains(NSWindowCollectionBehavior::MoveToActiveSpace));''','''        assert!(behavior.contains(NSWindowCollectionBehavior::FullScreenAuxiliary));
        assert!(behavior.contains(NSWindowCollectionBehavior::CanJoinAllApplications));
        assert!(!behavior.contains(NSWindowCollectionBehavior::MoveToActiveSpace));''')
edit('docs/architecture.md','''On macOS, transient capture, countdown, recording-control, ripple, and
completion windows explicitly join other applications' full-screen Spaces.
Display coordinates use the fixed Core Graphics main-display baseline rather
than the current key window's screen.''','''On macOS, transient capture, countdown, recording-control, ripple, and
completion windows explicitly join other applications' full-screen Spaces.
After applying the full-screen collection behavior they are reordered at their
existing high window level without activating Kiri. In particular, a global
capture shortcut pressed over a full-screen video must not switch back to
Kiri's ordinary Space merely to present the capture overlay. The overlay
becomes normally interactive when clicked. Display coordinates use the fixed
Core Graphics main-display baseline rather than the current key window's
screen.''')
p=R/'docs/adr/0029-macos-fullscreen-shortcut-overlay.md';assert not p.exists();p.write_text('''# ADR 0029: Preserve the frontmost macOS full-screen Space for shortcut capture

- Status: Accepted
- Date: 2026-09-07

## Problem

Kiri already marked capture windows as eligible for other applications' native
full-screen Spaces. The global shortcut nevertheless focused the new WebView
and then activated Kiri. When a browser or player owned a full-screen Space,
that activation could switch away from the content before the overlay became
usable. A window created before its collection behavior was updated could also
remain behind the full-screen owner.

## Decision

For macOS transient capture windows, apply `CanJoinAllSpaces`,
`CanJoinAllApplications`, and `FullScreenAuxiliary`, keep the existing high
window level, and call AppKit `orderFrontRegardless` after applying that policy.
The call is deliberately non-activating. Do not call Tauri `set_focus` or
activate Kiri during macOS capture-overlay creation. The user's click on the
overlay establishes normal interaction focus. Windows keeps its existing
explicit focus/activation path.

Countdown, recording controls, completion feedback and click ripple use the
same non-activating ordering rule so they remain visible in the captured
application's full-screen Space without stealing that Space.

## Verification

Rust policy tests require the full-screen and cross-application collection
flags. macOS CI compiles both architectures and runs the platform tests.
Windows CI must remain unchanged. A final native acceptance check should press
the global shortcut while a video/browser is in macOS native full screen and
confirm that the overlay appears on that same Space; CI cannot prove Mission
Control/Space switching behavior.
''')
subprocess.run(['git','diff','--check'],check=True)
