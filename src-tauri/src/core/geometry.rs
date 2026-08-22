//! Rectangle geometry shared across capture, library, and recording. Canvas
//! coordinates use a top-left origin while Quartz screen coordinates use a
//! bottom-left origin. Selection and annotation operations live in the
//! frontend; this module provides the portable data type.

/// A rectangle in the same conventions as CGRect: (x, y) is the top-left
/// (or bottom-left, depending on the coordinate system in use) corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}
