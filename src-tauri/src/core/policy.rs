//! Platform-independent recording policy and option validation.
//! Values define the stable recording/export contract across platforms.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecordingOutputFormat {
    #[default]
    Mp4,
    Gif,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingOptions {
    #[serde(default)]
    pub output_format: RecordingOutputFormat,
    pub uses_countdown: bool,
    pub captures_system_audio: bool,
    pub captures_microphone: bool,
    pub shows_cursor: bool,
    pub highlights_clicks: bool,
}

impl Default for RecordingOptions {
    fn default() -> Self {
        Self {
            output_format: RecordingOutputFormat::Mp4,
            uses_countdown: true,
            captures_system_audio: false,
            captures_microphone: false,
            shows_cursor: true,
            highlights_clicks: false,
        }
    }
}

impl RecordingOptions {
    /// Mirrors `RecordingOptions.normalized`: disabling the cursor also
    /// disables the click highlight.
    pub fn normalized(mut self) -> Self {
        if !self.shows_cursor {
            self.highlights_clicks = false;
        }
        self
    }
}

pub struct RecordingPolicy;

impl RecordingPolicy {
    pub const FRAMES_PER_SECOND: u32 = 30;
    pub const GIF_FRAMES_PER_SECOND: u32 = 12;
    pub const MAXIMUM_GIF_LONG_EDGE: u32 = 720;

    /// Mirrors `RecordingPolicy.evenDimension`.
    pub fn even_dimension(value: i64) -> i64 {
        let positive = value.max(2);
        if positive % 2 == 0 {
            positive
        } else {
            positive - 1
        }
    }

    /// Mirrors `RecordingPolicy.pixelDimension(points:backingScale:)`.
    pub fn pixel_dimension(points: f64, backing_scale: f64) -> i64 {
        Self::even_dimension((points * backing_scale.max(1.0)).round() as i64)
    }

    /// Mirrors `RecordingPolicy.highQualityBitRate(width:height:)`.
    pub fn high_quality_bit_rate(width: i64, height: i64) -> i64 {
        let pixel_based_rate = width.max(1) * height.max(1) * 8;
        pixel_based_rate.clamp(4_000_000, 40_000_000)
    }

    /// Mirrors `RecordingPolicy.isGIFEligible(duration:)`.
    pub fn is_gif_eligible(duration: Option<f64>) -> bool {
        match duration {
            Some(d) => d > 0.0,
            None => false,
        }
    }

    /// Mirrors `RecordingPolicy.elapsedLabel(_:)`.
    pub fn elapsed_label(duration: f64) -> String {
        let total_seconds = (duration.max(0.0).floor()) as u64;
        let hours = total_seconds / 3600;
        let minutes = (total_seconds % 3600) / 60;
        let seconds = total_seconds % 60;
        if hours > 0 {
            format!("{hours}:{minutes:02}:{seconds:02}")
        } else {
            format!("{minutes:02}:{seconds:02}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn even_dimension_keeps_even_and_floors_odd() {
        assert_eq!(RecordingPolicy::even_dimension(100), 100);
        assert_eq!(RecordingPolicy::even_dimension(101), 100);
        assert_eq!(RecordingPolicy::even_dimension(0), 2);
        assert_eq!(RecordingPolicy::even_dimension(-5), 2);
    }

    #[test]
    fn pixel_dimension_scales_and_floors_to_even() {
        assert_eq!(RecordingPolicy::pixel_dimension(500.0, 2.0), 1000);
        assert_eq!(RecordingPolicy::pixel_dimension(500.5, 2.0), 1000);
        assert_eq!(RecordingPolicy::pixel_dimension(1.0, 0.5), 2);
    }

    #[test]
    fn bit_rate_clamps_to_policy_range() {
        assert_eq!(
            RecordingPolicy::high_quality_bit_rate(1920, 1080),
            16_588_800
        );
        assert_eq!(RecordingPolicy::high_quality_bit_rate(100, 100), 4_000_000);
        assert_eq!(
            RecordingPolicy::high_quality_bit_rate(4000, 4000),
            40_000_000
        );
    }

    #[test]
    fn gif_eligibility_accepts_any_positive_duration() {
        assert!(RecordingPolicy::is_gif_eligible(Some(0.5)));
        assert!(RecordingPolicy::is_gif_eligible(Some(15.0)));
        assert!(RecordingPolicy::is_gif_eligible(Some(3_600.0)));
        assert!(!RecordingPolicy::is_gif_eligible(Some(0.0)));
        assert!(!RecordingPolicy::is_gif_eligible(None));
    }

    #[test]
    fn elapsed_label_formats_like_swift() {
        assert_eq!(RecordingPolicy::elapsed_label(0.0), "00:00");
        assert_eq!(RecordingPolicy::elapsed_label(65.9), "01:05");
        assert_eq!(RecordingPolicy::elapsed_label(3600.0), "1:00:00");
    }

    #[test]
    fn normalized_options_disable_highlights_without_cursor() {
        let options = RecordingOptions {
            shows_cursor: false,
            highlights_clicks: true,
            ..Default::default()
        };
        let normalized = options.normalized();
        assert!(!normalized.highlights_clicks);
    }

    #[test]
    fn recording_output_format_serializes_for_frontend_ipc() {
        let options = RecordingOptions {
            output_format: RecordingOutputFormat::Gif,
            ..Default::default()
        };
        let json = serde_json::to_value(options).unwrap();
        assert_eq!(json["outputFormat"], serde_json::json!("gif"));
        assert_eq!(
            serde_json::to_value(RecordingOutputFormat::Mp4).unwrap(),
            serde_json::json!("mp4")
        );
    }

    #[test]
    fn legacy_recording_options_default_to_mp4() {
        let options: RecordingOptions = serde_json::from_str(
            r#"{
                "usesCountdown": false,
                "capturesSystemAudio": true,
                "capturesMicrophone": true,
                "showsCursor": true,
                "highlightsClicks": false
            }"#,
        )
        .unwrap();

        assert_eq!(options.output_format, RecordingOutputFormat::Mp4);
        assert!(options.captures_system_audio);
        assert!(options.captures_microphone);
    }

    #[test]
    fn gif_normalization_preserves_saved_audio_preferences() {
        let options = RecordingOptions {
            output_format: RecordingOutputFormat::Gif,
            captures_system_audio: true,
            captures_microphone: true,
            ..Default::default()
        };

        let normalized = options.normalized();
        assert!(normalized.captures_system_audio);
        assert!(normalized.captures_microphone);
    }
}
