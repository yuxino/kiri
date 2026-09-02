#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(windows)]
pub mod windows;

#[cfg(target_os = "macos")]
pub use macos as current;

#[cfg(windows)]
pub use windows as current;

use crate::core::geometry::Rect;
use std::sync::Arc;

#[derive(Default)]
struct CaptureHealthState {
    expected_stop: bool,
    unexpected_failure: Option<String>,
}

/// Shared status for native capture callbacks whose failure arrives outside
/// the synchronous `stop()` call. The mutex makes the boundary atomic: an OS
/// close callback racing a user-requested stop is either retained as a real
/// failure or ignored as the expected shutdown, never both.
#[derive(Default)]
pub(crate) struct CaptureHealth {
    state: std::sync::Mutex<CaptureHealthState>,
}

impl CaptureHealth {
    pub(crate) fn begin_expected_stop(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.expected_stop = true;
    }

    /// Returns true when this is the first unexpected native-stop failure.
    pub(crate) fn report_unexpected_stop(&self, message: String) -> bool {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.expected_stop || state.unexpected_failure.is_some() {
            return false;
        }
        state.unexpected_failure = Some(message);
        true
    }

    pub(crate) fn unexpected_failure(&self) -> Option<String> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .unexpected_failure
            .clone()
    }
}

/// Raw Retina frames are very large (about 32 MiB at 4K). Keep only a tiny
/// hand-off queue between native capture and the platform encoder; callbacks
/// drop a frame when the encoder is behind instead of growing memory without
/// bound or blocking the operating system's capture thread.
pub const VIDEO_FRAME_QUEUE_CAPACITY: usize = 2;
pub type VideoFrameSender = std::sync::mpsc::SyncSender<Vec<u8>>;

/// Windows monitor-layout signature captured together with the frozen selection.
/// The one-based enumeration index is not stable when monitor topology
/// changes, so recording revalidates every field immediately before WGC starts.
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayIdentity {
    pub device_name: String,
    pub physical_x: i32,
    pub physical_y: i32,
    pub physical_width: u32,
    pub physical_height: u32,
    pub scale_factor: f64,
}

#[cfg(any(windows, test))]
pub(crate) fn unique_display_identity_index(
    expected: &DisplayIdentity,
    candidates: &[DisplayIdentity],
) -> Option<usize> {
    let mut matches = candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| *candidate == expected)
        .map(|(index, _)| index);
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

/// A frozen display capture plus the geometry the overlay needs.
#[derive(Debug, Clone)]
pub struct CapturedDisplay {
    /// PNG bytes at Retina resolution (top-left oriented).
    /// Shared with the custom protocol and OCR preparation so the full frozen
    /// image has one backing allocation for the lifetime of a capture.
    pub png_data: Arc<[u8]>,
    /// Pixel dimensions of the captured image.
    pub pixel_width: i64,
    pub pixel_height: i64,
    /// Global display frame in points (top-left orientation, y down).
    pub screen_frame: Rect,
    /// Display-local window rectangles in points, front-to-back,
    /// already converted to top-left orientation.
    pub window_rects: Vec<Rect>,
    /// Platform display identifier (CGDirectDisplayID on macOS; one-based
    /// initial EnumDisplayMonitors position on Windows, retained only for
    /// diagnostics; Windows recording uses `display_identity` instead).
    pub display_id: u32,
    /// Current Windows layout signature used to survive enumeration reorder
    /// while rejecting disconnection, DPI/geometry changes, and ambiguity.
    pub display_identity: Option<DisplayIdentity>,
    /// Backing scale factor (>= 1).
    pub backing_scale: f64,
}

/// Converts one Windows monitor's physical virtual-desktop bounds into the
/// logical coordinates expected by Tauri's initial window position and size.
/// Keeping the monitor origin is essential: otherwise a frozen secondary
/// display is shown by an overlay created at the primary display's `(0, 0)`.
#[cfg(any(windows, test))]
fn logical_monitor_frame(x: i32, y: i32, width: u32, height: u32, scale: f64) -> Rect {
    let scale = scale.max(1.0);
    Rect::new(
        f64::from(x) / scale,
        f64::from(y) / scale,
        f64::from(width) / scale,
        f64::from(height) / scale,
    )
}

/// Platform recording session (SCK stream on macOS, WGC + WASAPI on Windows).
pub trait PlatformRecorder: Send {
    fn stop(&mut self) -> anyhow::Result<()>;

    /// Windows keeps one Media Foundation encoder open while capture is
    /// paused so resuming never requires segment concatenation.
    fn pause(&mut self) -> anyhow::Result<()> {
        anyhow::bail!("This capture backend does not support in-place pause.")
    }

    fn resume(&mut self) -> anyhow::Result<()> {
        anyhow::bail!("This capture backend does not support in-place resume.")
    }
}

#[cfg(test)]
mod tests {
    use super::{
        logical_monitor_frame, unique_display_identity_index, CaptureHealth, DisplayIdentity,
    };

    fn identity(name: &str, x: i32, scale: f64) -> DisplayIdentity {
        DisplayIdentity {
            device_name: name.into(),
            physical_x: x,
            physical_y: 0,
            physical_width: 1920,
            physical_height: 1080,
            scale_factor: scale,
        }
    }

    #[test]
    fn native_capture_failure_survives_a_later_stop_request() {
        let health = CaptureHealth::default();
        assert!(health.report_unexpected_stop("display disappeared".into()));
        health.begin_expected_stop();
        assert_eq!(
            health.unexpected_failure().as_deref(),
            Some("display disappeared")
        );
    }

    #[test]
    fn native_close_during_an_expected_stop_is_not_a_failure() {
        let health = CaptureHealth::default();
        health.begin_expected_stop();
        assert!(!health.report_unexpected_stop("normal close callback".into()));
        assert!(health.unexpected_failure().is_none());
    }

    #[test]
    fn windows_monitor_frame_keeps_secondary_display_origin() {
        assert_eq!(
            logical_monitor_frame(-2560, 360, 2560, 1440, 2.0),
            crate::core::geometry::Rect::new(-1280.0, 180.0, 1280.0, 720.0)
        );
    }

    #[test]
    fn display_identity_survives_enumeration_reordering() {
        let selected = identity(r"\\.\DISPLAY2", 1920, 1.5);
        let reordered = [selected.clone(), identity(r"\\.\DISPLAY1", 0, 1.0)];
        assert_eq!(
            unique_display_identity_index(&selected, &reordered),
            Some(0)
        );
    }

    #[test]
    fn display_identity_fails_closed_on_disconnect_or_topology_change() {
        let selected = identity(r"\\.\DISPLAY2", 1920, 1.5);
        assert_eq!(
            unique_display_identity_index(&selected, &[identity(r"\\.\DISPLAY1", 0, 1.0)]),
            None
        );
        for changed in [
            identity(r"\\.\DISPLAY2", 0, 1.5),
            identity(r"\\.\DISPLAY2", 1920, 1.25),
            DisplayIdentity {
                physical_width: 2560,
                ..selected.clone()
            },
        ] {
            assert_eq!(unique_display_identity_index(&selected, &[changed]), None);
        }
    }

    #[test]
    fn display_identity_fails_closed_on_ambiguous_exact_matches() {
        let selected = identity(r"\\.\DISPLAY2", 1920, 1.5);
        assert_eq!(
            unique_display_identity_index(&selected, &[selected.clone(), selected.clone()]),
            None
        );
    }
}
