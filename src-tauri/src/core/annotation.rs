//! Versioned, bounded annotation documents shared by capture staging, the
//! editor, and the on-disk sidecar. The Swift-compatible asset index remains
//! deliberately unaware of this format.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

pub const ANNOTATION_SCHEMA_VERSION: u8 = 1;
pub const MAX_ANNOTATION_DOCUMENT_BYTES: usize = 4 * 1024 * 1024;
const MAX_CANVAS_DIMENSION: f64 = 65_536.0;
const MAX_MARKS: usize = 2_048;
const MAX_POINTS_PER_MARK: usize = 100_000;
const MAX_TOTAL_POINTS: usize = 100_000;
const MAX_TOTAL_TEXT_UNITS: usize = 65_536;
const MAX_VISUAL_SIZE: f64 = 4_096.0;
const MAX_COORDINATE_MAGNITUDE: f64 = 262_144.0;
const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnnotationDocument {
    pub schema_version: u8,
    pub canvas: AnnotationSize,
    pub source_pixels: AnnotationPixelSize,
    pub marks: Vec<AnnotationMark>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnnotationSize {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnnotationPixelSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnnotationPoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnnotationRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnnotationColor {
    Violet,
    Cherry,
    Orange,
    Yellow,
    Mint,
    Blue,
    White,
    Black,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextBackground {
    Transparent,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MosaicIntensity {
    Soft,
    Standard,
    Strong,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MosaicStyle {
    Pixel,
    Blur,
}

/// Last-used annotation styling shared by the capture overlay and editor.
/// The active tool is deliberately excluded so every new surface still opens
/// in its predictable selection state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct AnnotationAppearance {
    pub color_preset: AnnotationColor,
    pub text_background_style: TextBackground,
    pub mosaic_intensity: MosaicIntensity,
    pub mosaic_style: MosaicStyle,
    pub pen_width: u16,
    pub shape_width: u16,
    pub text_font_size: u16,
    pub mosaic_brush_diameter: u16,
}

impl Default for AnnotationAppearance {
    fn default() -> Self {
        Self {
            color_preset: AnnotationColor::Violet,
            text_background_style: TextBackground::Transparent,
            mosaic_intensity: MosaicIntensity::Standard,
            mosaic_style: MosaicStyle::Pixel,
            pen_width: 3,
            shape_width: 3,
            text_font_size: 18,
            mosaic_brush_diameter: 20,
        }
    }
}

impl AnnotationAppearance {
    pub fn normalized(mut self) -> Self {
        self.pen_width = self.pen_width.clamp(1, 24);
        self.shape_width = self.shape_width.clamp(1, 16);
        self.text_font_size = self.text_font_size.clamp(12, 64);
        self.mosaic_brush_diameter = self.mosaic_brush_diameter.clamp(12, 120);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "lowercase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum AnnotationMark {
    Pen {
        id: f64,
        points: Vec<AnnotationPoint>,
        color: AnnotationColor,
        width: f64,
    },
    Rectangle {
        id: f64,
        rect: AnnotationRect,
        color: AnnotationColor,
        width: f64,
    },
    Line {
        id: f64,
        start: AnnotationPoint,
        end: AnnotationPoint,
        color: AnnotationColor,
        width: f64,
    },
    Arrow {
        id: f64,
        start: AnnotationPoint,
        end: AnnotationPoint,
        color: AnnotationColor,
        width: f64,
    },
    Text {
        id: f64,
        text: String,
        rect: AnnotationRect,
        color: AnnotationColor,
        background: TextBackground,
        font_size: f64,
    },
    Mosaic {
        id: f64,
        points: Vec<AnnotationPoint>,
        brush_diameter: f64,
        intensity: MosaicIntensity,
        style: MosaicStyle,
    },
}

impl AnnotationDocument {
    pub fn from_json(document_json: &str) -> Result<Self, String> {
        if document_json.is_empty() || document_json.len() > MAX_ANNOTATION_DOCUMENT_BYTES {
            return Err("The annotation document is too large.".into());
        }
        let document: Self = serde_json::from_str(document_json)
            .map_err(|_| "The annotation document is invalid.".to_string())?;
        document.validate()?;
        Ok(document)
    }

    pub fn to_json(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)
            .map_err(|_| "The annotation document could not be encoded.".to_string())?;
        if bytes.len() > MAX_ANNOTATION_DOCUMENT_BYTES {
            return Err("The annotation document is too large.".into());
        }
        Ok(bytes)
    }

    pub fn has_marks(&self) -> bool {
        !self.marks.is_empty()
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != ANNOTATION_SCHEMA_VERSION {
            return Err("The annotation document version is unsupported.".into());
        }
        validate_dimension(self.canvas.width)?;
        validate_dimension(self.canvas.height)?;
        if self.source_pixels.width == 0
            || self.source_pixels.height == 0
            || f64::from(self.source_pixels.width) > MAX_CANVAS_DIMENSION
            || f64::from(self.source_pixels.height) > MAX_CANVAS_DIMENSION
        {
            return Err("The annotation source dimensions are invalid.".into());
        }
        if self.marks.len() > MAX_MARKS {
            return Err("The annotation document contains too many marks.".into());
        }

        let coordinate_limit = MAX_COORDINATE_MAGNITUDE;
        let mut ids = HashSet::with_capacity(self.marks.len());
        let mut total_points = 0usize;
        let mut total_text_bytes = 0usize;
        for mark in &self.marks {
            let (id, points, text_bytes) = match mark {
                AnnotationMark::Pen {
                    id, points, width, ..
                } => {
                    validate_visual_size(*width)?;
                    validate_points(points, 1, coordinate_limit)?;
                    (*id, points.len(), 0)
                }
                AnnotationMark::Rectangle {
                    id, rect, width, ..
                } => {
                    validate_visual_size(*width)?;
                    validate_rect(*rect, coordinate_limit, true)?;
                    (*id, 0, 0)
                }
                AnnotationMark::Line {
                    id,
                    start,
                    end,
                    width,
                    ..
                }
                | AnnotationMark::Arrow {
                    id,
                    start,
                    end,
                    width,
                    ..
                } => {
                    validate_visual_size(*width)?;
                    validate_point(*start, coordinate_limit)?;
                    validate_point(*end, coordinate_limit)?;
                    (*id, 0, 0)
                }
                AnnotationMark::Text {
                    id,
                    text,
                    rect,
                    font_size,
                    ..
                } => {
                    if text.encode_utf16().count() > MAX_TOTAL_TEXT_UNITS {
                        return Err("The annotation text is invalid.".into());
                    }
                    validate_rect(*rect, coordinate_limit, true)?;
                    validate_visual_size(*font_size)?;
                    (*id, 0, text.encode_utf16().count())
                }
                AnnotationMark::Mosaic {
                    id,
                    points,
                    brush_diameter,
                    ..
                } => {
                    validate_visual_size(*brush_diameter)?;
                    validate_points(points, 1, coordinate_limit)?;
                    (*id, points.len(), 0)
                }
            };

            let normalized_id = if id == 0.0 { 0.0 } else { id };
            if !id.is_finite()
                || !(0.0..=MAX_SAFE_INTEGER).contains(&id)
                || !ids.insert(normalized_id.to_bits())
            {
                return Err("The annotation mark id is invalid.".into());
            }
            total_points = total_points
                .checked_add(points)
                .ok_or_else(|| "The annotation document contains too many points.".to_string())?;
            total_text_bytes = total_text_bytes
                .checked_add(text_bytes)
                .ok_or_else(|| "The annotation document contains too much text.".to_string())?;
            if total_points > MAX_TOTAL_POINTS {
                return Err("The annotation document contains too many points.".into());
            }
            if total_text_bytes > MAX_TOTAL_TEXT_UNITS {
                return Err("The annotation document contains too much text.".into());
            }
        }
        Ok(())
    }

    /// Validates the document against the persisted image that owns it. The
    /// generic schema bounds above are not enough: a well-formed sidecar with
    /// different source dimensions or aspect ratio must be treated as invalid
    /// instead of being paired with the wrong pixels.
    pub fn validate_for_image_pixels(
        &self,
        expected_width: i64,
        expected_height: i64,
    ) -> Result<(), String> {
        self.validate()?;
        if expected_width <= 0
            || expected_height <= 0
            || u32::try_from(expected_width).ok() != Some(self.source_pixels.width)
            || u32::try_from(expected_height).ok() != Some(self.source_pixels.height)
        {
            return Err("The annotation document does not match the edited image.".into());
        }
        let document_ratio = self.canvas.width / self.canvas.height;
        let source_ratio = expected_width as f64 / expected_height as f64;
        if !document_ratio.is_finite()
            || (document_ratio - source_ratio).abs() > source_ratio.abs().max(1.0) * 0.005
        {
            return Err("The annotation canvas does not match the edited image.".into());
        }
        Ok(())
    }
}

fn validate_dimension(value: f64) -> Result<(), String> {
    if value.is_finite() && value > 0.0 && value <= MAX_CANVAS_DIMENSION {
        Ok(())
    } else {
        Err("The annotation canvas dimensions are invalid.".into())
    }
}

fn validate_visual_size(value: f64) -> Result<(), String> {
    if value.is_finite() && value > 0.0 && value <= MAX_VISUAL_SIZE {
        Ok(())
    } else {
        Err("The annotation mark size is invalid.".into())
    }
}

fn validate_point(point: AnnotationPoint, limit: f64) -> Result<(), String> {
    if point.x.is_finite()
        && point.y.is_finite()
        && point.x.abs() <= limit
        && point.y.abs() <= limit
    {
        Ok(())
    } else {
        Err("The annotation point is invalid.".into())
    }
}

fn validate_points(
    points: &[AnnotationPoint],
    minimum: usize,
    coordinate_limit: f64,
) -> Result<(), String> {
    if points.len() < minimum || points.len() > MAX_POINTS_PER_MARK {
        return Err("The annotation point list is invalid.".into());
    }
    points
        .iter()
        .try_for_each(|point| validate_point(*point, coordinate_limit))
}

fn validate_rect(rect: AnnotationRect, limit: f64, allow_zero_size: bool) -> Result<(), String> {
    let minimum = if allow_zero_size { 0.0 } else { f64::EPSILON };
    if rect.x.is_finite()
        && rect.y.is_finite()
        && rect.width.is_finite()
        && rect.height.is_finite()
        && rect.x.abs() <= limit
        && rect.y.abs() <= limit
        && rect.width >= minimum
        && rect.height >= minimum
        && rect.width <= limit
        && rect.height <= limit
    {
        Ok(())
    } else {
        Err("The annotation rectangle is invalid.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document_json(mark: &str) -> String {
        format!(
            r#"{{"schemaVersion":1,"canvas":{{"width":100,"height":80}},"sourcePixels":{{"width":200,"height":160}},"marks":[{mark}]}}"#
        )
    }

    #[test]
    fn parses_every_supported_mark_and_round_trips() {
        let marks = [
            r#"{"kind":"pen","id":1,"points":[{"x":1,"y":2},{"x":3,"y":4}],"color":"violet","width":3}"#,
            r#"{"kind":"rectangle","id":2,"rect":{"x":1,"y":2,"width":30,"height":20},"color":"cherry","width":3}"#,
            r#"{"kind":"line","id":3,"start":{"x":1,"y":2},"end":{"x":3,"y":4},"color":"orange","width":3}"#,
            r#"{"kind":"arrow","id":4,"start":{"x":1,"y":2},"end":{"x":3,"y":4},"color":"yellow","width":3}"#,
            r#"{"kind":"text","id":5,"text":"Kiri","rect":{"x":1,"y":2,"width":30,"height":20},"color":"mint","background":"transparent","fontSize":18}"#,
            r#"{"kind":"mosaic","id":6,"points":[{"x":1,"y":2}],"brushDiameter":20,"intensity":"standard","style":"pixel"}"#,
        ];
        let json = format!(
            r#"{{"schemaVersion":1,"canvas":{{"width":100,"height":80}},"sourcePixels":{{"width":200,"height":160}},"marks":[{}]}}"#,
            marks.join(",")
        );
        let parsed = AnnotationDocument::from_json(&json).unwrap();
        let encoded = String::from_utf8(parsed.to_json().unwrap()).unwrap();
        assert_eq!(AnnotationDocument::from_json(&encoded).unwrap(), parsed);
    }

    #[test]
    fn rejects_unknown_fields_kinds_enums_and_duplicate_ids() {
        assert!(AnnotationDocument::from_json(&document_json(
            r#"{"kind":"pen","id":1,"points":[{"x":1,"y":2},{"x":3,"y":4}],"color":"violet","width":3,"extra":true}"#
        ))
        .is_err());
        assert!(
            AnnotationDocument::from_json(&document_json(r#"{"kind":"sparkle","id":1}"#)).is_err()
        );
        assert!(AnnotationDocument::from_json(&document_json(
            r#"{"kind":"pen","id":1,"points":[{"x":1,"y":2},{"x":3,"y":4}],"color":"cyan","width":3}"#
        ))
        .is_err());
        let duplicate = document_json(
            r#"{"kind":"line","id":1,"start":{"x":1,"y":2},"end":{"x":3,"y":4},"color":"blue","width":3},{"kind":"arrow","id":1,"start":{"x":1,"y":2},"end":{"x":3,"y":4},"color":"white","width":3}"#,
        );
        assert!(AnnotationDocument::from_json(&duplicate).is_err());
        let signed_zero_duplicate = document_json(
            r#"{"kind":"line","id":0,"start":{"x":1,"y":2},"end":{"x":3,"y":4},"color":"blue","width":3},{"kind":"arrow","id":-0,"start":{"x":1,"y":2},"end":{"x":3,"y":4},"color":"white","width":3}"#,
        );
        assert!(AnnotationDocument::from_json(&signed_zero_duplicate).is_err());
    }

    #[test]
    fn rejects_non_finite_programmatic_geometry_and_oversized_json() {
        let mut document = AnnotationDocument::from_json(&document_json(
            r#"{"kind":"rectangle","id":1,"rect":{"x":1,"y":2,"width":30,"height":20},"color":"black","width":3}"#,
        ))
        .unwrap();
        document.canvas.width = f64::NAN;
        assert!(document.validate().is_err());

        let oversized = " ".repeat(MAX_ANNOTATION_DOCUMENT_BYTES + 1);
        assert!(AnnotationDocument::from_json(&oversized).is_err());
    }

    #[test]
    fn validates_source_dimensions_and_canvas_ratio_against_the_image() {
        let document = AnnotationDocument::from_json(
            r#"{"schemaVersion":1,"canvas":{"width":100,"height":80},"sourcePixels":{"width":200,"height":160},"marks":[]}"#,
        )
        .unwrap();
        document.validate_for_image_pixels(200, 160).unwrap();
        assert!(document.validate_for_image_pixels(201, 160).is_err());

        let mut wrong_ratio = document;
        wrong_ratio.canvas.width = 120.0;
        assert!(wrong_ratio.validate_for_image_pixels(200, 160).is_err());
    }

    #[test]
    fn annotation_appearance_defaults_and_clamps_visual_sizes() {
        let defaults: AnnotationAppearance = serde_json::from_str("{}").unwrap();
        assert_eq!(defaults, AnnotationAppearance::default());

        let oversized: AnnotationAppearance = serde_json::from_str(
            r#"{
                "colorPreset":"cherry",
                "textBackgroundStyle":"dark",
                "mosaicIntensity":"strong",
                "mosaicStyle":"blur",
                "penWidth":0,
                "shapeWidth":99,
                "textFontSize":2,
                "mosaicBrushDiameter":999
            }"#,
        )
        .unwrap();
        let normalized = oversized.normalized();
        assert_eq!(normalized.color_preset, AnnotationColor::Cherry);
        assert_eq!(normalized.text_background_style, TextBackground::Dark);
        assert_eq!(normalized.mosaic_intensity, MosaicIntensity::Strong);
        assert_eq!(normalized.mosaic_style, MosaicStyle::Blur);
        assert_eq!(normalized.pen_width, 1);
        assert_eq!(normalized.shape_width, 16);
        assert_eq!(normalized.text_font_size, 12);
        assert_eq!(normalized.mosaic_brush_diameter, 120);
        assert!(serde_json::from_str::<AnnotationAppearance>(r#"{"extra":true}"#).is_err());
    }
}
