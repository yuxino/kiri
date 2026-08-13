//! SelectionGeometry — exact port of Sources/KiriCore/SelectionGeometry.swift.
//! Uses the same conventions as the Swift version: top-left origin for
//! canvas/view coordinates, bottom-left origin for Quartz screen coordinates.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SelectionHandle {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
}

impl SelectionHandle {
    pub const ALL: [SelectionHandle; 8] = [
        SelectionHandle::TopLeft,
        SelectionHandle::Top,
        SelectionHandle::TopRight,
        SelectionHandle::Right,
        SelectionHandle::BottomRight,
        SelectionHandle::Bottom,
        SelectionHandle::BottomLeft,
        SelectionHandle::Left,
    ];
}

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
    pub const NULL: Rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
    };

    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn min_x(&self) -> f64 {
        self.x
    }

    pub fn min_y(&self) -> f64 {
        self.y
    }

    pub fn max_x(&self) -> f64 {
        self.x + self.width
    }

    pub fn max_y(&self) -> f64 {
        self.y + self.height
    }

    pub fn mid_x(&self) -> f64 {
        self.x + self.width / 2.0
    }

    pub fn mid_y(&self) -> f64 {
        self.y + self.height / 2.0
    }

    pub fn contains(&self, point: Point) -> bool {
        point.x >= self.min_x()
            && point.x <= self.max_x()
            && point.y >= self.min_y()
            && point.y <= self.max_y()
    }

    pub fn is_null(&self) -> bool {
        self.width == 0.0 && self.height == 0.0
    }

    /// Mirrors `CGRect.standardized` (non-negative width/height).
    pub fn standardized(&self) -> Rect {
        Rect {
            x: if self.width >= 0.0 {
                self.x
            } else {
                self.x + self.width
            },
            y: if self.height >= 0.0 {
                self.y
            } else {
                self.y + self.height
            },
            width: self.width.abs(),
            height: self.height.abs(),
        }
    }

    /// Mirrors `CGRect.intersection`.
    pub fn intersection(&self, other: &Rect) -> Rect {
        let min_x = self.min_x().max(other.min_x());
        let min_y = self.min_y().max(other.min_y());
        let max_x = self.max_x().min(other.max_x());
        let max_y = self.max_y().min(other.max_y());
        if max_x < min_x || max_y < min_y {
            Rect::NULL
        } else {
            Rect::new(min_x, min_y, max_x - min_x, max_y - min_y)
        }
    }

    /// Mirrors `CGRect.integral`.
    pub fn integral(&self) -> Rect {
        let min_x = self.min_x().floor();
        let min_y = self.min_y().floor();
        let max_x = self.max_x().ceil();
        let max_y = self.max_y().ceil();
        Rect::new(min_x, min_y, max_x - min_x, max_y - min_y)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

impl Size {
    pub fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }
}

pub struct SelectionGeometry;

impl SelectionGeometry {
    /// Mirrors `SelectionGeometry.normalized(from:to:)`.
    pub fn normalized(from: Point, to: Point) -> Rect {
        Rect::new(
            from.x.min(to.x),
            from.y.min(to.y),
            (to.x - from.x).abs(),
            (to.y - from.y).abs(),
        )
    }

    /// Mirrors `SelectionGeometry.clamped(_:to:)`.
    pub fn clamped(rect: &Rect, bounds: &Rect) -> Rect {
        rect.standardized().intersection(bounds)
    }

    /// Mirrors `SelectionGeometry.isValid(_:minimumSide:)`.
    pub fn is_valid(rect: &Rect, minimum_side: f64) -> bool {
        !rect.is_null() && rect.width >= minimum_side && rect.height >= minimum_side
    }

    /// Mirrors `SelectionGeometry.pixelRect(forTopLeftRect:canvasSize:imageSize:)`.
    pub fn pixel_rect_for_top_left(
        rect: &Rect,
        canvas_size: Size,
        image_size: Size,
    ) -> Rect {
        if canvas_size.width <= 0.0 || canvas_size.height <= 0.0 {
            return Rect::NULL;
        }
        let scale_x = image_size.width / canvas_size.width;
        let scale_y = image_size.height / canvas_size.height;
        Rect::new(
            rect.min_x() * scale_x,
            rect.min_y() * scale_y,
            rect.width * scale_x,
            rect.height * scale_y,
        )
        .integral()
    }

    /// Mirrors `SelectionGeometry.pixelRect(forScreenRect:displayFrame:scale:)`.
    pub fn pixel_rect_for_screen(rect: &Rect, display_frame: &Rect, scale: f64) -> Rect {
        Rect::new(
            (rect.min_x() - display_frame.min_x()) * scale,
            (display_frame.max_y() - rect.max_y()) * scale,
            rect.width * scale,
            rect.height * scale,
        )
        .integral()
    }

    /// Mirrors `SelectionGeometry.handlePoint(for:in:)`.
    pub fn handle_point(handle: SelectionHandle, selection: &Rect) -> Point {
        let rect = selection.standardized();
        match handle {
            SelectionHandle::TopLeft => Point::new(rect.min_x(), rect.min_y()),
            SelectionHandle::Top => Point::new(rect.mid_x(), rect.min_y()),
            SelectionHandle::TopRight => Point::new(rect.max_x(), rect.min_y()),
            SelectionHandle::Right => Point::new(rect.max_x(), rect.mid_y()),
            SelectionHandle::BottomRight => Point::new(rect.max_x(), rect.max_y()),
            SelectionHandle::Bottom => Point::new(rect.mid_x(), rect.max_y()),
            SelectionHandle::BottomLeft => Point::new(rect.min_x(), rect.max_y()),
            SelectionHandle::Left => Point::new(rect.min_x(), rect.mid_y()),
        }
    }

    /// Mirrors `SelectionGeometry.hitTest(_:selection:radius:)`.
    pub fn hit_test(point: Point, selection: &Rect, radius: f64) -> Option<SelectionHandle> {
        if !Self::is_valid(selection, 3.0) || radius < 0.0 {
            return None;
        }
        SelectionHandle::ALL.iter().copied().find(|handle| {
            let center = Self::handle_point(*handle, selection);
            let dx = point.x - center.x;
            let dy = point.y - center.y;
            (dx * dx + dy * dy).sqrt() <= radius
        })
    }

    /// Mirrors `SelectionGeometry.resized(_:using:to:within:minimumSide:)`.
    pub fn resized(
        selection: &Rect,
        handle: SelectionHandle,
        point: Point,
        bounds: &Rect,
        minimum_side: f64,
    ) -> Rect {
        let rect = selection.standardized();
        let limits = bounds.standardized();
        let minimum = minimum_side.max(1.0);
        let clamped_point = Point::new(
            point.x.clamp(limits.min_x(), limits.max_x()),
            point.y.clamp(limits.min_y(), limits.max_y()),
        );

        let mut min_x = rect.min_x();
        let mut max_x = rect.max_x();
        let mut min_y = rect.min_y();
        let mut max_y = rect.max_y();

        match handle {
            SelectionHandle::TopLeft | SelectionHandle::Left | SelectionHandle::BottomLeft => {
                min_x = clamped_point.x.min(max_x - minimum);
            }
            SelectionHandle::TopRight | SelectionHandle::Right | SelectionHandle::BottomRight => {
                max_x = clamped_point.x.max(min_x + minimum);
            }
            SelectionHandle::Top | SelectionHandle::Bottom => {}
        }

        match handle {
            SelectionHandle::TopLeft | SelectionHandle::Top | SelectionHandle::TopRight => {
                min_y = clamped_point.y.min(max_y - minimum);
            }
            SelectionHandle::BottomLeft | SelectionHandle::Bottom | SelectionHandle::BottomRight => {
                max_y = clamped_point.y.max(min_y + minimum);
            }
            SelectionHandle::Left | SelectionHandle::Right => {}
        }

        min_x = min_x.max(limits.min_x());
        max_x = max_x.min(limits.max_x());
        min_y = min_y.max(limits.min_y());
        max_y = max_y.min(limits.max_y());

        Rect::new(min_x, min_y, max_x - min_x, max_y - min_y)
    }

    /// Mirrors `SelectionGeometry.moved(_:by:within:)`.
    pub fn moved(selection: &Rect, translation: Size, bounds: &Rect) -> Rect {
        let rect = selection.standardized();
        let limits = bounds.standardized();
        if rect.width > limits.width || rect.height > limits.height {
            return Self::clamped(&rect, &limits);
        }
        let x = (rect.min_x() + translation.width)
            .max(limits.min_x())
            .min(limits.max_x() - rect.width);
        let y = (rect.min_y() + translation.height)
            .max(limits.min_y())
            .min(limits.max_y() - rect.height);
        Rect::new(x, y, rect.width, rect.height)
    }
}

pub struct WindowSelectionGeometry;

impl WindowSelectionGeometry {
    /// Mirrors `WindowSelectionGeometry.candidate(at:windowsFrontToBack:within:minimumSide:)`.
    pub fn candidate(
        point: Point,
        windows_front_to_back: &[Rect],
        bounds: &Rect,
        minimum_side: f64,
    ) -> Option<Rect> {
        let display_bounds = bounds.standardized();
        let minimum = minimum_side.max(1.0);
        for window in windows_front_to_back {
            let visible = window.standardized().intersection(&display_bounds);
            if !visible.is_null()
                && visible.width >= minimum
                && visible.height >= minimum
                && visible.contains(point)
            {
                return Some(visible);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect::new(x, y, w, h)
    }

    fn point(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn normalizes_reverse_drag() {
        let r = SelectionGeometry::normalized(point(120.0, 90.0), point(20.0, 10.0));
        assert_eq!(r, rect(20.0, 10.0, 100.0, 80.0));
    }

    #[test]
    fn clamps_selection_to_display() {
        let result = SelectionGeometry::clamped(
            &rect(-20.0, 30.0, 80.0, 90.0),
            &rect(0.0, 0.0, 100.0, 100.0),
        );
        assert_eq!(result, rect(0.0, 30.0, 60.0, 70.0));
    }

    #[test]
    fn converts_top_left_view_rect_to_pixels() {
        let result = SelectionGeometry::pixel_rect_for_top_left(
            &rect(50.0, 25.0, 100.0, 50.0),
            Size::new(200.0, 100.0),
            Size::new(400.0, 200.0),
        );
        assert_eq!(result, rect(100.0, 50.0, 200.0, 100.0));
    }

    #[test]
    fn converts_bottom_left_screen_coordinates_to_top_left_pixels() {
        let result = SelectionGeometry::pixel_rect_for_screen(
            &rect(110.0, 220.0, 50.0, 40.0),
            &rect(100.0, 200.0, 300.0, 200.0),
            2.0,
        );
        assert_eq!(result, rect(20.0, 280.0, 100.0, 80.0));
    }

    #[test]
    fn rejects_tiny_selection() {
        assert!(!SelectionGeometry::is_valid(&rect(0.0, 0.0, 2.0, 20.0), 3.0));
        assert!(SelectionGeometry::is_valid(&rect(0.0, 0.0, 3.0, 3.0), 3.0));
    }

    #[test]
    fn hit_tests_selection_handles() {
        let selection = rect(20.0, 30.0, 100.0, 80.0);
        assert_eq!(
            SelectionGeometry::hit_test(point(20.0, 30.0), &selection, 6.0),
            Some(SelectionHandle::TopLeft)
        );
        assert_eq!(
            SelectionGeometry::hit_test(point(70.0, 30.0), &selection, 6.0),
            Some(SelectionHandle::Top)
        );
        assert_eq!(
            SelectionGeometry::hit_test(point(120.0, 70.0), &selection, 6.0),
            Some(SelectionHandle::Right)
        );
        assert_eq!(
            SelectionGeometry::hit_test(point(70.0, 70.0), &selection, 6.0),
            None
        );
    }

    #[test]
    fn resizes_selection_from_handles() {
        let selection = rect(10.0, 10.0, 100.0, 80.0);
        let bounds = rect(0.0, 0.0, 200.0, 200.0);

        let expanded = SelectionGeometry::resized(
            &selection,
            SelectionHandle::TopLeft,
            point(0.0, 5.0),
            &bounds,
            8.0,
        );
        assert_eq!(expanded, rect(0.0, 5.0, 110.0, 85.0));

        let minimum = SelectionGeometry::resized(
            &selection,
            SelectionHandle::Left,
            point(180.0, 50.0),
            &bounds,
            8.0,
        );
        assert_eq!(minimum, rect(102.0, 10.0, 8.0, 80.0));
    }

    #[test]
    fn moves_selection_within_bounds() {
        let selection = rect(10.0, 20.0, 50.0, 40.0);
        let bounds = rect(0.0, 0.0, 200.0, 200.0);
        let moved = SelectionGeometry::moved(&selection, Size::new(-50.0, 100.0), &bounds);
        assert_eq!(moved, rect(0.0, 120.0, 50.0, 40.0));
    }

    #[test]
    fn converts_selections_on_offset_displays() {
        let left_display = rect(-1440.0, 0.0, 1440.0, 900.0);
        let left_result = SelectionGeometry::pixel_rect_for_screen(
            &rect(-1400.0, 100.0, 200.0, 150.0),
            &left_display,
            2.0,
        );
        assert_eq!(left_result, rect(80.0, 1300.0, 400.0, 300.0));

        let upper_display = rect(0.0, 900.0, 1920.0, 1080.0);
        let upper_result = SelectionGeometry::pixel_rect_for_screen(
            &rect(100.0, 1000.0, 300.0, 200.0),
            &upper_display,
            1.0,
        );
        assert_eq!(upper_result, rect(100.0, 780.0, 300.0, 200.0));
    }

    #[test]
    fn window_click_chooses_topmost_candidate() {
        let front = rect(40.0, 30.0, 160.0, 120.0);
        let back = rect(10.0, 10.0, 260.0, 180.0);
        let result = WindowSelectionGeometry::candidate(
            point(80.0, 70.0),
            &[front, back],
            &rect(0.0, 0.0, 300.0, 200.0),
            8.0,
        );
        assert_eq!(result, Some(front));
    }

    #[test]
    fn window_click_clips_candidate_to_display() {
        let result = WindowSelectionGeometry::candidate(
            point(10.0, 60.0),
            &[rect(-40.0, 20.0, 120.0, 100.0)],
            &rect(0.0, 0.0, 200.0, 150.0),
            8.0,
        );
        assert_eq!(result, Some(rect(0.0, 20.0, 80.0, 100.0)));
    }

    #[test]
    fn window_click_filters_invalid_candidates() {
        let bounds = rect(0.0, 0.0, 300.0, 200.0);
        let result = WindowSelectionGeometry::candidate(
            point(100.0, 100.0),
            &[
                rect(99.0, 99.0, 2.0, 2.0),
                rect(320.0, 20.0, 100.0, 100.0),
                rect(60.0, 60.0, 120.0, 90.0),
            ],
            &bounds,
            8.0,
        );
        assert_eq!(result, Some(rect(60.0, 60.0, 120.0, 90.0)));

        let missing = WindowSelectionGeometry::candidate(
            point(10.0, 10.0),
            &[rect(60.0, 60.0, 120.0, 90.0)],
            &bounds,
            8.0,
        );
        assert_eq!(missing, None);
    }
}
