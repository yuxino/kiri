//! Geometry — the rectangle type shared across capture, library, and
//! recording. Coordinates follow the Swift original's conventions: top-left
//! origin for canvas/view coordinates, bottom-left origin for Quartz screen
//! coordinates. Selection/annotation geometry itself lives in the frontend
//! (src/annotation/geom.ts); this crate only needs the plain data type.

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
