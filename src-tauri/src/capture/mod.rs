#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(windows)]
pub mod windows;

#[cfg(target_os = "macos")]
pub use macos as current;

#[cfg(windows)]
pub use windows as current;

use crate::core::geometry::Rect;

/// A frozen display capture plus the geometry the overlay needs.
#[derive(Debug, Clone)]
pub struct CapturedDisplay {
    /// PNG bytes at Retina resolution (top-left oriented).
    pub png_data: Vec<u8>,
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

/// Optional capture capabilities offered while recording.
#[derive(Debug, Clone, Copy, Default)]
pub struct RecordingSources {
    pub system_audio: bool,
    pub microphone: bool,
}

/// Platform recording session (SCK stream on macOS, WGC + WASAPI on Windows).
pub trait PlatformRecorder: Send {
    fn stop(&mut self) -> anyhow::Result<()>;
}
