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
/// hand-off queue between native capture and the FFmpeg pipe writer; callbacks
/// drop a frame when the encoder is behind instead of growing memory without
/// bound or blocking the operating system's capture thread.
pub const VIDEO_FRAME_QUEUE_CAPACITY: usize = 2;
pub type VideoFrameSender = std::sync::mpsc::SyncSender<Vec<u8>>;

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
    /// Platform display identifier (CGDirectDisplayID on macOS).
    pub display_id: u32,
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
}

#[cfg(test)]
mod tests {
    use super::{logical_monitor_frame, CaptureHealth};

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
}
