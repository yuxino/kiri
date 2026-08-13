//! Windows capture backend — xcap (WGC) for screenshots and window
//! enumeration. Recording arrives in the Windows milestone.

use anyhow::{bail, Result};

use crate::core::geometry::Rect;

use super::{CapturedDisplay, PlatformRecorder};

pub fn capture_active_display() -> Result<CapturedDisplay> {
    bail!("Windows capture backend not implemented yet")
}

pub struct WindowsRecorder;

impl WindowsRecorder {
    pub fn start(
        _display_id: u32,
        _region: Rect,
        _backing_scale: f64,
    ) -> Result<WindowsRecorder> {
        bail!("Windows recorder not implemented yet")
    }
}

impl PlatformRecorder for WindowsRecorder {
    fn stop(&mut self) -> Result<()> {
        Ok(())
    }
}
