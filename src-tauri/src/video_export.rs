//! Native, non-destructive MP4 trimming and size presets. Run on a worker thread.
use anyhow::{bail, Context, Result};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

pub const EXPORT_CANCELLED: &str = "VIDEO_EXPORT_CANCELLED";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportPhase {
    Preparing,
    Rendering,
    Saving,
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct ExportProgress {
    pub phase: ExportPhase,
    pub progress: Option<f64>,
}

struct ExportControlInner {
    // Cancellation and library import have one atomic linearization point.
    // Once committing begins, cancellation reports false and import completes.
    state: AtomicU8,
    reported: Mutex<Option<(ExportProgress, Instant)>>,
    observer: Box<dyn Fn(ExportProgress) + Send + Sync>,
}

#[derive(Clone)]
pub struct ExportControl(Arc<ExportControlInner>);

impl Default for ExportControl {
    fn default() -> Self {
        Self::new(|_| {})
    }
}

impl ExportControl {
    pub fn new(observer: impl Fn(ExportProgress) + Send + Sync + 'static) -> Self {
        Self(Arc::new(ExportControlInner {
            state: AtomicU8::new(0),
            reported: Mutex::new(None),
            observer: Box::new(observer),
        }))
    }

    pub fn request_cancel(&self) -> bool {
        self.0
            .state
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
            || self.is_cancelled()
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.state.load(Ordering::Acquire) == 1
    }

    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            bail!(EXPORT_CANCELLED);
        }
        Ok(())
    }

    pub fn begin_commit(&self) -> Result<()> {
        self.0
            .state
            .compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| anyhow::anyhow!(EXPORT_CANCELLED))?;
        Ok(())
    }

    pub fn report(&self, phase: ExportPhase, progress: Option<f64>) {
        if self.is_cancelled() {
            return;
        }
        let progress = progress
            .filter(|value| value.is_finite())
            .map(|value| value.clamp(0.0, 1.0));
        let mut reported = self
            .0
            .reported
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if self.is_cancelled() {
            return;
        }
        let mut update = ExportProgress { phase, progress };
        if let Some((previous, time)) = *reported {
            if phase < previous.phase {
                return;
            }
            if phase == previous.phase {
                // Native pipelines can contain multiple passes or out-of-order callbacks.
                // Never let a stale callback move the displayed work backwards.
                if let Some(previous) = previous.progress {
                    update.progress = Some(progress.unwrap_or(previous).max(previous));
                }
                if update.progress == previous.progress
                    || (update.progress != Some(1.0) && time.elapsed() < Duration::from_millis(100))
                {
                    return;
                }
            }
        }
        *reported = Some((update, Instant::now()));
        // Serialize delivery along with the watermark so concurrent native callbacks
        // cannot emit a later event first. Observers must be brief and non-reentrant.
        (self.0.observer)(update);
    }

    pub(super) fn rendering(&self) -> ExportProgressRange {
        ExportProgressRange {
            control: self.clone(),
            start: 0.0,
            end: 1.0,
        }
    }
}

/// Measured pass progress mapped onto a fixed share of the complete render.
/// This is processed work, not an estimated clock or remaining-time promise.
#[derive(Clone)]
pub(super) struct ExportProgressRange {
    control: ExportControl,
    start: f64,
    end: f64,
}

impl ExportProgressRange {
    pub(super) fn check(&self) -> Result<()> {
        self.control.check()
    }
    pub(super) fn report(&self, progress: f64) {
        if progress.is_finite() {
            self.control.report(
                ExportPhase::Rendering,
                Some(self.start + (self.end - self.start) * progress.clamp(0.0, 1.0)),
            );
        }
    }
    #[cfg(windows)]
    fn child(&self, start: f64, end: f64) -> Self {
        Self {
            control: self.control.clone(),
            start: self.start + (self.end - self.start) * start,
            end: self.start + (self.end - self.start) * end,
        }
    }
}

/// A source can be on a slow external disk. Copy in bounded chunks, outside the
/// library mutex, checking cancellation between reads and writes.
pub fn copy_export_source<R: std::io::Read, W: std::io::Write>(
    source: &mut R,
    destination: &mut W,
    length: u64,
    control: &ExportControl,
) -> Result<()> {
    let mut buffer = vec![0; 1024 * 1024];
    let mut copied = 0;
    control.report(ExportPhase::Preparing, Some(0.0));
    while copied < length {
        control.check()?;
        let size = usize::try_from((length - copied).min(buffer.len() as u64))?;
        let count = source.read(&mut buffer[..size])?;
        if count == 0 {
            bail!("The video source changed while preparing export");
        }
        control.check()?;
        destination.write_all(&buffer[..count])?;
        copied += count as u64;
        control.report(ExportPhase::Preparing, Some(copied as f64 / length as f64));
    }
    control.check()?;
    Ok(())
}

#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoExportPreset {
    Original,
    Share,
    Small,
}

impl VideoExportPreset {
    fn max_edge(self) -> u32 {
        match self {
            Self::Original => 0,
            Self::Share => 1080,
            Self::Small => 720,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, serde::Deserialize)]
pub struct VideoSegment {
    pub start: f64,
    pub end: f64,
    #[serde(default = "default_segment_speed")]
    pub speed: f64,
}

fn default_segment_speed() -> f64 {
    1.0
}
impl Default for VideoSegment {
    fn default() -> Self {
        Self {
            start: 0.0,
            end: 0.0,
            speed: 1.0,
        }
    }
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoEffectKind {
    Zoom,
    Mask,
    Spotlight,
    Frame,
    Fade,
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoMaskStyle {
    #[default]
    Solid,
    Blur,
    Pixelate,
}

fn default_effect_strength() -> f64 {
    0.5
}

fn default_layer() -> i32 { -1 }

#[repr(C)]
#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoEffect {
    pub kind: VideoEffectKind,
    pub start: f64,
    pub end: f64,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub mask_style: VideoMaskStyle,
    #[serde(default = "default_effect_strength")]
    pub strength: f64,
    #[serde(default)]
    pub transition: f64,
    #[serde(default)]
    pub color: u32,
    #[serde(default = "default_layer")]
    pub layer: i32,
}

impl Default for VideoEffect {
    fn default() -> Self {
        Self {
            kind: VideoEffectKind::Zoom,
            start: 0.0,
            end: 0.0,
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
            mask_style: VideoMaskStyle::Solid,
            strength: default_effect_strength(),
            transition: 0.0,
            color: 0,
            layer: default_layer(),
        }
    }
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoAnnotationKind {
    Overlay,
    Pixelate,
    Blur,
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoAnnotation {
    pub start: f64,
    pub end: f64,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub kind: VideoAnnotationKind,
    pub image_base64: String,
    pub amount: f64,
    #[serde(default = "default_layer")]
    pub layer: i32,
}

pub(super) struct PreparedVideoAnnotation {
    pub start: f64,
    pub end: f64,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub kind: VideoAnnotationKind,
    pub image: image::RgbaImage,
    pub amount: f64,
    pub layer: i32,
}

/// Overlay tracks composite back to front, then whole-picture adjustments.
/// Omitted order fields retain the historical rendering of older callers.
pub(super) fn composition_order(effects: &[VideoEffect], annotations: &[PreparedVideoAnnotation]) -> Vec<usize> {
    let mut layers: Vec<_> = annotations.iter().enumerate().map(|(index, a)| {
        (0, if a.layer >= 0 {a.layer} else if a.kind == VideoAnnotationKind::Overlay {1} else {0}, index)
    }).chain(effects.iter().enumerate().map(|(index, effect)| {
        let (group, fallback) = match effect.kind {
            VideoEffectKind::Mask => (0, 3), VideoEffectKind::Spotlight => (0, 4),
            VideoEffectKind::Zoom => (1, 5), VideoEffectKind::Frame => (1, 6), VideoEffectKind::Fade => (1, 7),
        };
        (group, if effect.layer >= 0 {effect.layer} else {fallback}, annotations.len() + index)
    })).collect();
    layers.sort_by_key(|&(group, order, index)| (group, order, index));
    layers.into_iter().map(|(_, _, index)| index).collect()
}

#[cfg(test)]
fn prepare_annotations(annotations: &[VideoAnnotation]) -> Result<Vec<PreparedVideoAnnotation>> {
    prepare_annotations_controlled(annotations, &ExportControl::default())
}

fn prepare_annotations_controlled(annotations: &[VideoAnnotation], control: &ExportControl) -> Result<Vec<PreparedVideoAnnotation>> {
    use base64::Engine;
    use image::ImageDecoder;
    if annotations.len() > 128 {
        bail!("Too many video annotations");
    }
    let encoded_bytes = annotations
        .iter()
        .try_fold(0usize, |sum, annotation| {
            sum.checked_add(annotation.image_base64.len())
        })
        .context("Video annotation payload is too large")?;
    if encoded_bytes > 64 * 1024 * 1024 {
        bail!("Video annotation payload is too large");
    }
    let mut remaining = 128u64 * 1024 * 1024;
    annotations
        .iter()
        .map(|annotation| {
            control.check()?;
            let VideoAnnotation {
                start,
                end,
                x,
                y,
                width,
                height,
                amount,
                ..
            } = *annotation;
            if [start, end, x, y, width, height, amount]
                .iter()
                .any(|value| !value.is_finite())
                || start < 0.0
                || end <= start
                || x < 0.0
                || y < 0.0
                || width <= 0.0
                || height <= 0.0
                || x + width > 1.000001
                || y + height > 1.000001
                || amount < 0.0
                || amount > 1.0
                || (annotation.kind != VideoAnnotationKind::Overlay && amount <= 0.0)
            {
                bail!("Invalid video annotation timing, rectangle or amount");
            }
            let encoded = annotation
                .image_base64
                .strip_prefix("data:image/png;base64,")
                .unwrap_or(&annotation.image_base64);
            let png = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .context("Invalid video annotation base64")?;
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(4096);
            limits.max_image_height = Some(4096);
            limits.max_alloc = Some(remaining);
            let decoder =
                image::codecs::png::PngDecoder::with_limits(std::io::Cursor::new(png), limits)
                    .context("Invalid or oversized video annotation PNG")?;
            let (pixel_width, pixel_height) = decoder.dimensions();
            let size = u64::from(pixel_width) * u64::from(pixel_height) * 4;
            let peak_size = if decoder.color_type() == image::ColorType::Rgba8 {
                size
            } else {
                size.checked_add(decoder.total_bytes())
                    .context("Decoded video annotations are too large")?
            };
            if peak_size > remaining {
                bail!("Decoded video annotations are too large");
            }
            let image = image::DynamicImage::from_decoder(decoder)?.into_rgba8();
            remaining -= size;
            Ok(PreparedVideoAnnotation {
                start,
                end,
                x,
                y,
                width,
                height,
                amount,
                kind: annotation.kind,
                image,
                layer: annotation.layer,
            })
        })
        .collect()
}

fn validate_annotation_duration(
    annotations: &[PreparedVideoAnnotation],
    duration: f64,
) -> Result<()> {
    if !duration.is_finite()
        || duration <= 0.0
        || annotations
            .iter()
            .any(|annotation| annotation.start >= duration || annotation.end > duration + 0.05)
    {
        bail!("Video annotation is outside the source duration");
    }
    Ok(())
}

fn validate_effects(effects: &[VideoEffect], duration: Option<f64>) -> Result<()> {
    if effects.len() > 128 {
        bail!("Too many video effects");
    }
    for (index, effect) in effects.iter().enumerate() {
        let VideoEffect {
            start,
            end,
            x,
            y,
            width,
            height,
            ..
        } = *effect;
        if [start, end, x, y, width, height]
            .iter()
            .any(|value| !value.is_finite())
            || start < 0.0
            || end <= start
            || x < 0.0
            || y < 0.0
            || width < 0.02
            || height < 0.02
            || x + width > 1.000001
            || y + height > 1.000001
            || duration.is_some_and(|duration| end > duration + 0.05 || start >= duration)
        {
            bail!("Invalid video effect range or rectangle");
        }
        if !effect.strength.is_finite()
            || !(0.0..=1.0).contains(&effect.strength)
            || !effect.transition.is_finite()
            || !(0.0..=2.0).contains(&effect.transition)
            || effect.color > 0x00ff_ffff
        {
            bail!("Invalid video effect strength, transition or color");
        }
        if effect.kind == VideoEffectKind::Zoom {
            if (width - height).abs() > 0.000001 {
                bail!("Zoom must preserve the video aspect ratio");
            }
        }
        if matches!(
            effect.kind,
            VideoEffectKind::Zoom | VideoEffectKind::Frame | VideoEffectKind::Fade
        ) && effects[..index]
            .iter()
            .any(|other| other.kind == effect.kind && start < other.end && end > other.start)
        {
            bail!("Camera or fade effects of the same kind must not overlap");
        }
    }
    Ok(())
}

fn validate_segments(
    segments: &[VideoSegment],
    duration: Option<f64>,
) -> Result<Vec<VideoSegment>> {
    if segments.is_empty() || segments.len() > 128 {
        bail!("Video export requires between 1 and 128 segments");
    }
    if duration.is_some_and(|value| !value.is_finite() || value <= 0.0) {
        bail!("Invalid video duration");
    }
    let mut validated = Vec::with_capacity(segments.len());
    for &VideoSegment { start, end, speed } in segments {
        if !start.is_finite()
            || !end.is_finite()
            || start < 0.0
            || end <= start
            || !speed.is_finite()
            || !(0.25..=4.0).contains(&speed)
        {
            bail!("Invalid or overlapping video segments");
        }
        let end = if let Some(duration) = duration {
            if start >= duration || end > duration + 0.05 {
                bail!("Video segment is outside the source duration");
            }
            end.min(duration)
        } else {
            end
        };
        validated.push(VideoSegment { start, end, speed });
    }
    let mut ordered = validated.clone();
    ordered.sort_by(|a, b| a.start.total_cmp(&b.start));
    if ordered.windows(2).any(|pair| pair[0].end > pair[1].start) {
        bail!("Overlapping video segments");
    }
    Ok(validated)
}

/// The caller must remove the returned staging file after importing it.
/// A failed export removes all staging files and never modifies the source.
#[cfg(test)]
pub fn export_video(
    source: &Path,
    segments: &[VideoSegment],
    effects: &[VideoEffect],
    preset: VideoExportPreset,
) -> Result<(PathBuf, i64, i64, f64)> {
    export_video_with_annotations(source, segments, effects, &[], preset)
}

#[cfg(test)]
pub fn export_video_with_annotations(
    source: &Path,
    segments: &[VideoSegment],
    effects: &[VideoEffect],
    annotations: &[VideoAnnotation],
    preset: VideoExportPreset,
) -> Result<(PathBuf, i64, i64, f64)> {
    export_video_with_annotations_controlled(source, segments, effects, annotations, preset, &ExportControl::default())
}

pub fn export_video_with_annotations_controlled(
    source: &Path,
    segments: &[VideoSegment],
    effects: &[VideoEffect],
    annotations: &[VideoAnnotation],
    preset: VideoExportPreset,
    control: &ExportControl,
) -> Result<(PathBuf, i64, i64, f64)> {
    control.check()?;
    validate_segments(segments, None)?;
    validate_effects(effects, None)?;
    let annotations = prepare_annotations_controlled(annotations, control)?;
    let staging = tempfile::Builder::new()
        .prefix("kiri-video-export-")
        .tempdir()?;
    let output = staging.path().join("export.mp4");
    control.check()?;
    control.report(ExportPhase::Rendering, Some(0.0));
    let (width, height, duration) =
        platform_export(source, &output, segments, effects, &annotations, preset, &control.rendering())?;
    control.check()?;
    if width <= 0
        || height <= 0
        || !duration.is_finite()
        || duration <= 0.0
        || std::fs::metadata(&output)?.len() == 0
    {
        bail!("The native exporter produced an invalid video");
    }
    // Keep only the completed file; directory cleanup also handles native partial files.
    let destination =
        std::env::temp_dir().join(format!("kiri-export-{}.mp4", uuid::Uuid::new_v4()));
    std::fs::rename(&output, &destination).context("could not retain the exported video")?;
    // The returned file is owned by the caller from here, including cancellation
    // racing this rename. The command immediately adopts it into a TempPath.
    control.report(ExportPhase::Rendering, Some(1.0));
    Ok((destination, width, height, duration))
}

#[cfg(target_os = "macos")]
fn platform_export(
    source: &Path,
    output: &Path,
    segments: &[VideoSegment],
    effects: &[VideoEffect],
    annotations: &[PreparedVideoAnnotation],
    preset: VideoExportPreset,
    progress: &ExportProgressRange,
) -> Result<(i64, i64, f64)> {
    use std::{
        ffi::{c_char, c_void, CStr, CString},
        os::unix::ffi::OsStrExt,
    };
    #[repr(C)]
    struct NativeAnnotation {
        kind: VideoAnnotationKind,
        start: f64,
        end: f64,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        amount: f64,
        pixels: *const u8,
        pixel_width: u32,
        pixel_height: u32,
    }
    let native_annotations: Vec<_> = annotations
        .iter()
        .map(|annotation| NativeAnnotation {
            kind: annotation.kind,
            start: annotation.start,
            end: annotation.end,
            x: annotation.x,
            y: annotation.y,
            width: annotation.width,
            height: annotation.height,
            amount: annotation.amount,
            pixels: annotation.image.as_raw().as_ptr(),
            pixel_width: annotation.image.width(),
            pixel_height: annotation.image.height(),
        })
        .collect();
    let order = composition_order(effects, annotations);
    unsafe extern "C" {
        fn kiri_export_video(
            source: *const c_char,
            output: *const c_char,
            segments: *const VideoSegment,
            count: usize,
            effects: *const VideoEffect,
            effect_count: usize,
            annotations: *const NativeAnnotation,
            annotation_count: usize,
            order: *const usize,
            order_count: usize,
            max_edge: u32,
            progress: extern "C" fn(*const c_void, f64) -> bool,
            progress_context: *const c_void,
            error: *mut c_char,
            capacity: usize,
        ) -> bool;
    }
    extern "C" fn update_progress(context: *const c_void, fraction: f64) -> bool {
        // Do not unwind across AVFoundation's C bridge.
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let progress = unsafe { &*context.cast::<ExportProgressRange>() };
            if fraction >= 0.0 { progress.report(fraction); }
            progress.check().is_ok()
        })).unwrap_or(false)
    }
    progress.check()?;
    let (_, _, duration) = crate::macos_media::probe_media(source)?;
    let duration = duration.context("video duration is unavailable")?;
    let segments = validate_segments(segments, Some(duration))?;
    validate_effects(effects, Some(duration))?;
    validate_annotation_duration(annotations, duration)?;
    let source = CString::new(source.as_os_str().as_bytes())?;
    let destination = CString::new(output.as_os_str().as_bytes())?;
    let mut error = [0 as c_char; 1024];
    let success = unsafe {
        kiri_export_video(
            source.as_ptr(),
            destination.as_ptr(),
            segments.as_ptr(),
            segments.len(),
            effects.as_ptr(),
            effects.len(),
            native_annotations.as_ptr(),
            native_annotations.len(),
            order.as_ptr(),
            order.len(),
            preset.max_edge(),
            update_progress,
            (progress as *const ExportProgressRange).cast(),
            error.as_mut_ptr(),
            error.len(),
        )
    };
    progress.check()?;
    if !success {
        bail!(
            "{}",
            unsafe { CStr::from_ptr(error.as_ptr()) }.to_string_lossy()
        );
    }
    let (width, height, duration) = crate::macos_media::probe_media(output)?;
    progress.check()?;
    Ok((
        width,
        height,
        duration.context("exported duration is unavailable")?,
    ))
}

#[cfg(windows)]
#[path = "video_export_windows.rs"]
mod windows_export;
#[cfg(windows)]
use windows_export::platform_export;

#[cfg(not(any(target_os = "macos", windows)))]
fn platform_export(
    _: &Path,
    _: &Path,
    _: &[VideoSegment],
    _: &[VideoEffect],
    _: &[PreparedVideoAnnotation],
    _: VideoExportPreset,
    _: &ExportProgressRange,
) -> Result<(i64, i64, f64)> {
    bail!("Native video export is supported on macOS and Windows")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_control_cancels_before_reading_or_native_media_work() {
        let control = ExportControl::default();
        assert!(control.request_cancel());
        let result = export_video_with_annotations_controlled(Path::new("missing-source.mp4"),
            &[VideoSegment {start: 0.0, end: 1.0, speed: 1.0}], &[], &[], VideoExportPreset::Original, &control);
        assert_eq!(result.unwrap_err().to_string(), EXPORT_CANCELLED);
        let mut target = Vec::new();
        let result = copy_export_source(&mut std::io::Cursor::new(vec![7; 16]), &mut target, 16, &control);
        assert_eq!(result.unwrap_err().to_string(), EXPORT_CANCELLED);
        assert!(target.is_empty());
        assert!(control.begin_commit().is_err());
    }

    #[test]
    fn export_control_stops_source_copy_between_chunks() {
        struct Source { remaining: usize, control: ExportControl }
        impl std::io::Read for Source {
            fn read(&mut self, target: &mut [u8]) -> std::io::Result<usize> {
                let count = self.remaining.min(target.len());
                target[..count].fill(7);
                self.remaining -= count;
                if self.remaining <= 1024 * 1024 { self.control.request_cancel(); }
                Ok(count)
            }
        }
        let control = ExportControl::default();
        let mut source = Source { remaining: 3 * 1024 * 1024, control: control.clone() };
        let mut target = Vec::new();
        let result = copy_export_source(&mut source, &mut target, 3 * 1024 * 1024, &control);
        assert_eq!(result.unwrap_err().to_string(), EXPORT_CANCELLED);
        assert_eq!(target.len(), 1024 * 1024);
    }

    #[test]
    fn export_control_cancel_and_import_have_one_winner() {
        for _ in 0..32 {
            let control = ExportControl::default();
            let committing = control.clone();
            let ready = Arc::new(std::sync::Barrier::new(2));
            let worker_ready = ready.clone();
            let worker = std::thread::spawn(move || { worker_ready.wait(); committing.begin_commit().is_ok() });
            ready.wait();
            let cancelled = control.request_cancel();
            let committed = worker.join().unwrap();
            assert_ne!(cancelled, committed);
            if committed { assert!(!control.request_cancel()); }
            else { assert_eq!(control.check().unwrap_err().to_string(), EXPORT_CANCELLED); }
        }
    }

    #[test]
    fn export_control_progress_is_bounded_monotonic_and_throttled() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let observed = events.clone();
        let control = ExportControl::new(move |event| observed.lock().unwrap().push(event));
        control.report(ExportPhase::Preparing, None);
        control.report(ExportPhase::Rendering, Some(0.0));
        for index in 1..100 { control.report(ExportPhase::Rendering, Some(index as f64 / 100.0)); }
        control.report(ExportPhase::Rendering, Some(8.0));
        control.report(ExportPhase::Rendering, Some(0.5));
        control.report(ExportPhase::Preparing, Some(0.7));
        control.report(ExportPhase::Saving, None);
        control.report(ExportPhase::Rendering, Some(f64::NAN));
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 4, "per-frame callbacks must be throttled");
        assert_eq!(events[0].phase, ExportPhase::Preparing);
        assert_eq!(events[1].progress, Some(0.0));
        assert_eq!(events[2].progress, Some(1.0));
        assert_eq!(events[3].phase, ExportPhase::Saving);
        assert_eq!(events[3].progress, None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_export_cancel_stops_in_flight_render_and_keeps_source() {
        use crate::macos_media::MacosSegmentEncoder;
        use std::sync::{atomic::AtomicBool, OnceLock, Weak};
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("cancel-source.mp4");
        let mut encoder = MacosSegmentEncoder::new(&source, 1280, 720, 30, 4_000_000, false).unwrap();
        let frame: Vec<u8> = (0..1280 * 720).flat_map(|index| {
            [(index % 253) as u8, ((index / 1280) % 251) as u8, 90, 255]
        }).collect();
        for index in 0..180 { assert!(encoder.append_video(&frame, index).unwrap()); }
        encoder.finish().unwrap();
        let before = std::fs::read(&source).unwrap();
        let slot: Arc<OnceLock<Weak<ExportControlInner>>> = Arc::new(OnceLock::new());
        let observed = slot.clone();
        let cancelled_in_flight = Arc::new(AtomicBool::new(false));
        let triggered = cancelled_in_flight.clone();
        let control = ExportControl::new(move |event| {
            if event.phase == ExportPhase::Rendering && event.progress.is_some_and(|value| value > 0.0 && value < 1.0) {
                if let Some(inner) = observed.get().and_then(Weak::upgrade) {
                    triggered.store(ExportControl(inner).request_cancel(), Ordering::Release);
                }
            }
        });
        assert!(slot.set(Arc::downgrade(&control.0)).is_ok());
        let started = Instant::now();
        let result = export_video_with_annotations_controlled(&source,
            &[VideoSegment {start: 0.0, end: 6.0, speed: 1.0}],
            &[VideoEffect {kind: VideoEffectKind::Mask, start: 0.0, end: 6.0,
                mask_style: VideoMaskStyle::Blur, strength: 1.0, ..Default::default()}],
            &[], VideoExportPreset::Original, &control);
        assert!(cancelled_in_flight.load(Ordering::Acquire), "must cancel at real intermediate native progress");
        assert_eq!(result.unwrap_err().to_string(), EXPORT_CANCELLED);
        assert!(started.elapsed() < Duration::from_secs(30), "cancellation must finish promptly");
        assert_eq!(std::fs::read(&source).unwrap(), before);
    }
    #[test]
    fn rejects_invalid_and_out_of_bounds_ranges() {
        for (start, end, duration) in [
            (f64::NAN, 1.0, 2.0),
            (0.0, f64::INFINITY, 2.0),
            (-1.0, 1.0, 2.0),
            (1.0, 1.0, 2.0),
            (0.0, 3.0, 2.0),
            (2.0, 2.01, 2.0),
        ] {
            assert!(validate_segments(
                &[VideoSegment {
                    start,
                    end,
                    speed: 1.0
                }],
                Some(duration)
            )
            .is_err());
        }
        assert_eq!(
            validate_segments(
                &[VideoSegment {
                    start: 0.2,
                    end: 2.01,
                    speed: 1.0
                }],
                Some(2.0)
            )
            .unwrap()[0]
                .end,
            2.0
        );
    }

    #[test]
    fn allows_reordering_but_rejects_overlaps_and_excess_segments() {
        let range = VideoSegment {
            start: 0.0,
            end: 1.0,
            speed: 1.0,
        };
        assert!(validate_segments(&[], Some(3.0)).is_err());
        assert!(validate_segments(&[range; 129], Some(3.0)).is_err());
        assert!(validate_segments(&[range, range], Some(3.0)).is_err());
        assert!(validate_segments(
            &[
                VideoSegment {
                    start: 2.0,
                    end: 3.0,
                    speed: 1.0
                },
                range
            ],
            Some(3.0)
        )
        .is_ok());
        assert_eq!(
            validate_segments(
                &[
                    range,
                    VideoSegment {
                        start: 1.0,
                        end: 2.0,
                        speed: 1.0
                    }
                ],
                Some(3.0)
            )
            .unwrap()
            .len(),
            2
        );
    }

    #[test]
    fn segment_speed_defaults_and_limits() {
        let legacy: VideoSegment = serde_json::from_str(r#"{"start":0,"end":1}"#).unwrap();
        assert_eq!(legacy.speed, 1.0);
        for speed in [f64::NAN, f64::INFINITY, 0.0, -1.0, 0.249, 4.001] {
            assert!(validate_segments(&[VideoSegment { speed, ..legacy }], Some(2.0)).is_err());
        }
        for speed in [0.25, 0.5, 1.0, 2.0, 4.0] {
            assert!(validate_segments(&[VideoSegment { speed, ..legacy }], Some(2.0)).is_ok());
        }
        let reordered = validate_segments(
            &[
                VideoSegment {
                    start: 1.0,
                    end: 2.0,
                    speed: 2.0,
                },
                legacy,
            ],
            Some(2.0),
        )
        .unwrap();
        assert_eq!(reordered[0].start, 1.0);
        assert_eq!(reordered[1].speed, 1.0);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_mixed_speed_reordering_keeps_audio_and_source_timed_effects() {
        use crate::macos_media::MacosSegmentEncoder;
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("speed-source.mp4");
        let mut encoder = MacosSegmentEncoder::new(&source, 64, 48, 30, 500_000, true).unwrap();
        for index in 0..90 {
            let color = if index < 30 {
                [0, 0, 255, 255]
            } else {
                [255, 0, 0, 255]
            };
            let frame = color.repeat(64 * 48);
            assert!(encoder.append_video(&frame, index).unwrap());
        }
        let mut audio = Vec::with_capacity(3 * 48_000 * 4);
        for index in 0..3 * 48_000 {
            let frequency = if index < 48_000 { 440.0 } else { 660.0 };
            let sample = ((index as f64 * frequency * std::f64::consts::TAU / 48_000.0).sin()
                * 12_000.0) as i16;
            audio.extend_from_slice(&sample.to_le_bytes());
            audio.extend_from_slice(&sample.to_le_bytes());
        }
        encoder.append_audio(&audio).unwrap();
        encoder.finish().unwrap();
        let before = std::fs::read(&source).unwrap();
        let segments = [
            VideoSegment {
                start: 2.0,
                end: 3.0,
                speed: 2.0,
            },
            VideoSegment {
                start: 0.0,
                end: 1.0,
                speed: 0.5,
            },
        ];
        let effects = [VideoEffect {
            kind: VideoEffectKind::Mask,
            start: 2.2,
            end: 2.6,
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
            ..Default::default()
        }];
        let annotations = [VideoAnnotation {
            layer: -1,
            start: 0.25,
            end: 0.5,
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
            kind: VideoAnnotationKind::Overlay,
            amount: 0.0,
            image_base64: annotation_png(image::RgbaImage::from_pixel(
                2,
                2,
                image::Rgba([0, 255, 0, 255]),
            )),
        }];
        let (output, width, height, duration) = export_video_with_annotations(
            &source,
            &segments,
            &effects,
            &annotations,
            VideoExportPreset::Original,
        )
        .unwrap();
        assert_eq!((width, height), (64, 48));
        assert!((duration - 2.5).abs() < 0.06, "{duration}");
        assert!(frame_at(&output, 0.2)
            .get_pixel(32, 24)
            .0
            .iter()
            .all(|channel| *channel < 35));
        assert!(frame_at(&output, 0.4).get_pixel(32, 24)[2] > 180);
        assert!(frame_at(&output, 0.8).get_pixel(32, 24)[0] > 180);
        assert!(frame_at(&output, 1.2).get_pixel(32, 24)[1] > 180);
        assert!(frame_at(&output, 2.2).get_pixel(32, 24)[0] > 180);
        assert_audio_track(&output);
        for (probe_start, probe_end, expected_frequency) in [(0.1, 0.4, 660.0), (0.8, 2.2, 440.0)] {
            let (audio_start, audio_duration, frequency) =
                audio_stats(&output, probe_start, probe_end);
            assert!(audio_start.abs() < 0.05, "audio start {audio_start}");
            assert!(
                (audio_duration - 2.5).abs() < 0.08,
                "audio duration {audio_duration}"
            );
            assert!(
                (frequency - expected_frequency).abs() < 20.0,
                "pitch {frequency} vs {expected_frequency}"
            );
        }
        assert_eq!(before, std::fs::read(&source).unwrap());
        std::fs::remove_file(output).unwrap();
        // Source audio occupies only 0.5..2.0 seconds. The reordered first segment
        // has no audio, and slowing 0..1 to two seconds must delay its tone to 1.5.
        use std::ffi::{c_char, CString};
        unsafe extern "C" {
            fn kiri_test_video_delayed_audio(
                source: *const c_char,
                destination: *const c_char,
            ) -> bool;
        }
        let delayed = directory.path().join("delayed.mp4");
        let source_c = CString::new(source.to_str().unwrap()).unwrap();
        let delayed_c = CString::new(delayed.to_str().unwrap()).unwrap();
        assert!(unsafe { kiri_test_video_delayed_audio(source_c.as_ptr(), delayed_c.as_ptr()) });
        let (output, _, _, duration) =
            export_video(&delayed, &segments, &[], VideoExportPreset::Original).unwrap();
        assert!((duration - 2.5).abs() < 0.06);
        assert!(
            audio_stats(&output, 0.8, 1.2).2 < 10.0,
            "delayed audio must remain silent"
        );
        let frequency = audio_stats(&output, 1.7, 2.2).2;
        assert!(
            (frequency - 440.0).abs() < 20.0,
            "delayed pitch {frequency}"
        );
        std::fs::remove_file(output).unwrap();
    }

    #[test]
    fn validates_effect_rectangles_timing_and_zoom_overlap() {
        let effect = VideoEffect {
            kind: VideoEffectKind::Zoom,
            start: 0.0,
            end: 1.0,
            x: 0.0,
            y: 0.0,
            width: 0.5,
            height: 0.5,
            ..Default::default()
        };
        assert!(validate_effects(&[effect], Some(2.0)).is_ok());
        assert!(validate_effects(&[effect, effect], Some(2.0)).is_err());
        assert!(validate_effects(
            &[VideoEffect {
                width: 0.8,
                ..effect
            }],
            Some(2.0)
        )
        .is_err());
        assert!(validate_effects(&[VideoEffect { x: 0.8, ..effect }], Some(2.0)).is_err());
        assert!(validate_effects(&[VideoEffect { end: 3.0, ..effect }], Some(2.0)).is_err());
        assert!(validate_effects(
            &[VideoEffect {
                x: f64::NAN,
                ..effect
            }],
            Some(2.0)
        )
        .is_err());
    }

    #[test]
    fn legacy_effect_payloads_keep_solid_masks_and_hard_cut_zoom() {
        let effect: VideoEffect = serde_json::from_value(serde_json::json!({
            "kind":"mask", "start":0.0, "end":1.0, "x":0.0, "y":0.0, "width":0.5, "height":0.5
        }))
        .unwrap();
        assert_eq!(effect.mask_style, VideoMaskStyle::Solid);
        assert_eq!(
            (effect.strength, effect.transition, effect.color),
            (0.5, 0.0, 0)
        );
        let styled: VideoEffect = serde_json::from_value(serde_json::json!({
            "kind":"mask", "start":0.0, "end":1.0, "x":0.0, "y":0.0, "width":0.5, "height":0.5,
            "maskStyle":"pixelate", "strength":0.8, "transition":0.3, "color":3359829
        }))
        .unwrap();
        assert_eq!(styled.mask_style, VideoMaskStyle::Pixelate);
        assert_eq!(
            (styled.strength, styled.transition, styled.color),
            (0.8, 0.3, 3359829)
        );
        for invalid in [
            VideoEffect {
                strength: f64::NAN,
                ..effect
            },
            VideoEffect {
                strength: 1.1,
                ..effect
            },
            VideoEffect {
                transition: -0.1,
                ..effect
            },
            VideoEffect {
                transition: 2.1,
                ..effect
            },
            VideoEffect {
                color: 0x0100_0000,
                ..effect
            },
        ] {
            assert!(validate_effects(&[invalid], Some(2.0)).is_err());
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_mask_styles_use_live_frames_and_custom_solid_color() {
        use crate::macos_media::MacosSegmentEncoder;
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("mask-styles.mp4");
        let mut encoder = MacosSegmentEncoder::new(&source, 128, 96, 30, 2_000_000, false).unwrap();
        for index in 0..90 {
            let mut frame = vec![0u8; 128 * 96 * 4];
            for y in 0..96 {
                for x in 0..128 {
                    let channel = if index < 30 {
                        2
                    } else if index < 60 {
                        1
                    } else {
                        0
                    };
                    frame[(y * 128 + x) * 4 + channel] = if x % 4 < 2 { 255 } else { 90 };
                    frame[(y * 128 + x) * 4 + 3] = 255;
                }
            }
            assert!(encoder.append_video(&frame, index).unwrap());
        }
        encoder.finish().unwrap();
        for mask_style in [
            VideoMaskStyle::Solid,
            VideoMaskStyle::Blur,
            VideoMaskStyle::Pixelate,
        ] {
            let effect = VideoEffect {
                kind: VideoEffectKind::Mask,
                start: 0.5,
                end: 2.5,
                x: 0.0,
                y: 0.0,
                width: 0.5,
                height: 1.0,
                mask_style,
                strength: 1.0,
                color: 0x3366cc,
                ..Default::default()
            };
            let (output, _, _, _) = export_video(
                &source,
                &[VideoSegment {
                    start: 0.0,
                    end: 3.0,
                    speed: 1.0,
                }],
                &[effect],
                VideoExportPreset::Original,
            )
            .unwrap();
            let before = frame_at_with_edge(&output, 0.2, 128);
            assert!(before.get_pixel(24, 48)[0].abs_diff(before.get_pixel(26, 48)[0]) > 80);
            let first = frame_at_with_edge(&output, 0.7, 128);
            let second = frame_at_with_edge(&output, 1.7, 128);
            if mask_style == VideoMaskStyle::Solid {
                let actual = first.get_pixel(24, 48);
                assert!(
                    actual
                        .0
                        .iter()
                        .zip([51, 102, 204])
                        .all(|(actual, expected)| actual.abs_diff(expected) < 25),
                    "custom mask color: {actual:?}"
                );
            } else {
                assert!(
                    first.get_pixel(24, 48)[0] > 60 && first.get_pixel(24, 48)[1] < 50,
                    "{mask_style:?} red live frame {:?}",
                    first.get_pixel(24, 48)
                );
                assert!(
                    second.get_pixel(24, 48)[1] > 60 && second.get_pixel(24, 48)[0] < 50,
                    "{mask_style:?} green live frame {:?}",
                    second.get_pixel(24, 48)
                );
                assert!(
                    first.get_pixel(24, 48)[0].abs_diff(first.get_pixel(26, 48)[0]) < 60,
                    "{mask_style:?} must obscure the source texture"
                );
            }
            assert!(
                first.get_pixel(104, 48)[0].abs_diff(first.get_pixel(106, 48)[0]) > 80,
                "{mask_style:?} must not alter pixels outside its rectangle"
            );
            let after = frame_at_with_edge(&output, 2.7, 128);
            assert!(after.get_pixel(24, 48)[2].abs_diff(after.get_pixel(26, 48)[2]) > 80);
            std::fs::remove_file(output).unwrap();
        }
    }

    #[test]
    fn new_effects_keep_native_discriminants_and_validate_independent_intervals() {
        for (kind, value) in [("spotlight", 2), ("frame", 3), ("fade", 4)] {
            let effect: VideoEffect = serde_json::from_value(
                serde_json::json!({"kind":kind,"start":0,"end":3,"x":0,"y":0,"width":1,"height":1}),
            )
            .unwrap();
            assert_eq!(effect.kind as u32, value);
            assert!(validate_effects(&[effect], Some(3.0)).is_ok());
            assert_eq!(
                validate_effects(&[effect, effect], Some(3.0)).is_err(),
                kind != "spotlight"
            );
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_layer_reordering_changes_mask_and_annotation_occlusion() {
        use crate::macos_media::MacosSegmentEncoder;
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("layer-order.mp4");
        let mut encoder = MacosSegmentEncoder::new(&source, 96, 64, 30, 1_000_000, false).unwrap();
        let pixels: Vec<u8> = [0, 0, 255, 255].repeat(96 * 64);
        for index in 0..60 { assert!(encoder.append_video(&pixels, index).unwrap()); }
        encoder.finish().unwrap();
        let mask = VideoEffect { kind: VideoEffectKind::Mask, start: 0.5, end: 1.5,
            x: 0.25, y: 0.25, width: 0.5, height: 0.5, layer: 2, ..Default::default() };
        for (layer, expected_channel) in [(1, None), (3, Some(2))] {
            let overlay = VideoAnnotation { layer, start: 0.5, end: 1.5,
                x: 0.25, y: 0.25, width: 0.5, height: 0.5, kind: VideoAnnotationKind::Overlay,
                image_base64: annotation_png(image::RgbaImage::from_pixel(4, 4, image::Rgba([0, 0, 255, 255]))), amount: 0.0 };
            let (output, _, _, _) = export_video_with_annotations(&source,
                &[VideoSegment {start: 0.0, end: 2.0, speed: 1.0}], &[mask], &[overlay], VideoExportPreset::Original).unwrap();
            let frame = frame_at_with_edge(&output, 1.0, 96);
            let pixel = frame.get_pixel(48, 32);
            if let Some(channel) = expected_channel { assert!(pixel[channel] > 200, "upper annotation missing: {pixel:?}"); }
            else { assert!(pixel.0[..3].iter().all(|channel| *channel < 30), "lower annotation escaped mask: {pixel:?}"); }
            assert!(frame_at_with_edge(&output, 1.8, 96).get_pixel(48, 32)[0] > 200);
            std::fs::remove_file(output).unwrap();
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_spotlight_frame_and_fade_follow_live_source_and_timing() {
        use crate::macos_media::MacosSegmentEncoder;
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("presentation-effects.mp4");
        let mut encoder = MacosSegmentEncoder::new(&source, 128, 96, 30, 2_000_000, false).unwrap();
        for index in 0..90 {
            let mut pixels = vec![0u8; 128 * 96 * 4];
            for y in 0..96 {
                for x in 0..128 {
                    let channel = if x >= 64 {
                        0
                    } else if index < 45 {
                        2
                    } else {
                        1
                    };
                    pixels[(y * 128 + x) * 4 + channel] = 255;
                    pixels[(y * 128 + x) * 4 + 3] = 255;
                }
            }
            assert!(encoder.append_video(&pixels, index).unwrap());
        }
        encoder.finish().unwrap();
        let clip = VideoSegment {
            start: 0.0,
            end: 3.0,
            speed: 1.0,
        };
        for kind in [
            VideoEffectKind::Spotlight,
            VideoEffectKind::Frame,
            VideoEffectKind::Fade,
        ] {
            let effect = VideoEffect {
                kind,
                start: 0.0,
                end: 3.0,
                x: 0.0,
                y: 0.0,
                width: 0.5,
                height: 1.0,
                strength: 0.8,
                color: 0xffffff,
                transition: 0.5,
                ..Default::default()
            };
            let (output, _, _, _) =
                export_video(&source, &[clip], &[effect], VideoExportPreset::Original).unwrap();
            let first = frame_at_with_edge(&output, 0.75, 128);
            let second = frame_at_with_edge(&output, 2.0, 128);
            match kind {
                VideoEffectKind::Spotlight => {
                    assert!(first.get_pixel(30, 48)[0] > 200);
                    assert!(second.get_pixel(30, 48)[1] > 200);
                    assert!((25..80).contains(&first.get_pixel(110, 48)[2]));
                }
                VideoEffectKind::Frame => {
                    assert!(first.get_pixel(64, 48)[0] > 200);
                    assert!(second.get_pixel(64, 48)[1] > 200);
                    assert!(first.get_pixel(5, 5).0[..3].iter().all(|v| *v > 220));
                    assert!(first.get_pixel(64, 12).0[..3].iter().all(|v| *v > 220));
                }
                VideoEffectKind::Fade => {
                    let start = frame_at_with_edge(&output, 0.0, 128);
                    let halfway = frame_at_with_edge(&output, 0.25, 128);
                    assert!(start.get_pixel(30, 48).0[..3].iter().all(|v| *v > 220));
                    assert!((90..170).contains(&halfway.get_pixel(30, 48)[1]));
                    assert!(first.get_pixel(30, 48)[0] > 200 && first.get_pixel(30, 48)[1] < 40);
                    assert!(second.get_pixel(30, 48)[1] > 200 && second.get_pixel(30, 48)[0] < 40);
                }
                _ => unreachable!(),
            }
            std::fs::remove_file(output).unwrap();
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_zoom_eases_into_and_out_of_target_rectangle() {
        use crate::macos_media::MacosSegmentEncoder;
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("smooth-zoom.mp4");
        let mut encoder = MacosSegmentEncoder::new(&source, 128, 96, 30, 2_000_000, false).unwrap();
        let mut frame = vec![0u8; 128 * 96 * 4];
        for y in 0..96 {
            for x in 0..128 {
                frame[(y * 128 + x) * 4 + if x < 64 { 2 } else { 0 }] = 255;
                frame[(y * 128 + x) * 4 + 3] = 255;
            }
        }
        for index in 0..90 {
            assert!(encoder.append_video(&frame, index).unwrap());
        }
        encoder.finish().unwrap();
        // A short interval also tests transition being capped at half its duration.
        for (start, end, transition, middle, entering, leaving) in [
            (0.5, 2.5, 0.5, 1.5, 0.75, 2.25),
            (1.0, 2.0, 2.0, 1.5, 1.25, 1.75),
        ] {
            let effect = VideoEffect {
                kind: VideoEffectKind::Zoom,
                start,
                end,
                x: 0.0,
                y: 0.0,
                width: 0.5,
                height: 0.5,
                transition,
                ..Default::default()
            };
            let (output, _, _, _) = export_video(
                &source,
                &[VideoSegment {
                    start: 0.0,
                    end: 3.0,
                    speed: 1.0,
                }],
                &[effect],
                VideoExportPreset::Original,
            )
            .unwrap();
            let boundary = frame_at_with_edge(&output, start, 128);
            assert!(
                boundary.get_pixel(70, 48)[2] > 180,
                "zoom must begin at the full frame"
            );
            for time in [entering, leaving] {
                let ramp = frame_at_with_edge(&output, time, 128);
                assert!(
                    ramp.get_pixel(70, 48)[0] > 180 && ramp.get_pixel(110, 48)[2] > 180,
                    "zoom must pass through an intermediate rectangle at {time}"
                );
            }
            let peak = frame_at_with_edge(&output, middle, 128);
            assert!(
                peak.get_pixel(110, 48)[0] > 180,
                "zoom must reach its target"
            );
            let after = frame_at_with_edge(&output, end, 128);
            assert!(
                after.get_pixel(70, 48)[2] > 180,
                "zoom must return to the full frame"
            );
            std::fs::remove_file(output).unwrap();
        }
    }

    fn annotation_png(image: image::RgbaImage) -> String {
        use base64::Engine;
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        base64::engine::general_purpose::STANDARD.encode(png.into_inner())
    }

    #[test]
    fn annotation_payload_limits_and_geometry_are_validated() {
        let annotation = VideoAnnotation {
            layer: -1,
            start: 0.0,
            end: 1.0,
            x: 0.0,
            y: 0.0,
            width: 0.5,
            height: 0.5,
            kind: VideoAnnotationKind::Overlay,
            amount: 0.0,
            image_base64: annotation_png(image::RgbaImage::from_pixel(
                2,
                2,
                image::Rgba([255, 0, 0, 255]),
            )),
        };
        assert_eq!(
            prepare_annotations(&[annotation.clone()]).unwrap()[0]
                .image
                .dimensions(),
            (2, 2)
        );
        assert!(prepare_annotations(&vec![annotation.clone(); 129]).is_err());
        assert!(prepare_annotations(&[VideoAnnotation {
            width: f64::NAN,
            ..annotation.clone()
        }])
        .is_err());
        assert!(prepare_annotations(&[VideoAnnotation {
            image_base64: "not-png".into(),
            ..annotation.clone()
        }])
        .is_err());
        assert!(prepare_annotations(&[VideoAnnotation {
            kind: VideoAnnotationKind::Blur,
            ..annotation.clone()
        }])
        .is_err());
        let oversized = annotation_png(image::RgbaImage::new(4097, 1));
        assert!(prepare_annotations(&[VideoAnnotation {
            image_base64: oversized,
            ..annotation
        }])
        .is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_annotations_follow_live_frames_and_independent_time_ranges() {
        use crate::macos_media::MacosSegmentEncoder;
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("moving-stripes.mp4");
        let mut encoder = MacosSegmentEncoder::new(&source, 64, 48, 30, 1_000_000, false).unwrap();
        for index in 0..90 {
            let mut frame = vec![0u8; 64 * 48 * 4];
            for y in 0..48 {
                for x in 0..64 {
                    let channel = if index < 30 {
                        2
                    } else if index < 60 {
                        1
                    } else {
                        0
                    };
                    // Keep high-frequency stripes inside the mosaic region only. The
                    // transparency/orientation probes use the flat right half: otherwise
                    // repeated H.264 chroma subsampling creates red in bright green stripes,
                    // confounding an alpha-compositing assertion with codec reconstruction.
                    frame[(y * 64 + x) * 4 + channel] = if x >= 32 {
                        180
                    } else if x % 4 < 2 {
                        255
                    } else {
                        90
                    };
                    frame[(y * 64 + x) * 4 + 3] = 255;
                }
            }
            assert!(encoder.append_video(&frame, index).unwrap());
        }
        encoder.finish().unwrap();
        let coverage = annotation_png(image::RgbaImage::from_pixel(
            32,
            48,
            image::Rgba([255, 255, 255, 255]),
        ));
        // Asymmetric overlay additionally proves PNG rows retain top-left orientation.
        let mut overlay = image::RgbaImage::new(32, 24);
        for y in 0..12 {
            for x in 0..32 {
                overlay.put_pixel(x, y, image::Rgba([255, 255, 0, 255]));
            }
        }
        let overlay = annotation_png(overlay);
        for kind in [VideoAnnotationKind::Pixelate, VideoAnnotationKind::Blur] {
            let annotations = [
                VideoAnnotation {
                    layer: -1,
                    start: 0.5,
                    end: 2.5,
                    x: 0.0,
                    y: 0.0,
                    width: 0.5,
                    height: 1.0,
                    kind,
                    amount: 0.25,
                    image_base64: coverage.clone(),
                },
                VideoAnnotation {
                    layer: -1,
                    start: 1.0,
                    end: 2.0,
                    x: 0.5,
                    y: 0.0,
                    width: 0.5,
                    height: 0.5,
                    kind: VideoAnnotationKind::Overlay,
                    amount: 0.0,
                    image_base64: overlay.clone(),
                },
            ];
            let (output, _, _, _) = export_video_with_annotations(
                &source,
                &[VideoSegment {
                    start: 0.0,
                    end: 3.0,
                    speed: 1.0,
                }],
                &[],
                &annotations,
                VideoExportPreset::Original,
            )
            .unwrap();
            let before = frame_at(&output, 0.2);
            assert!(before.get_pixel(8, 24)[0].abs_diff(before.get_pixel(10, 24)[0]) > 80);
            let first = frame_at(&output, 0.7);
            assert!(first.get_pixel(8, 24)[0] > 70 && first.get_pixel(8, 24)[1] < 45);
            assert!(
                first.get_pixel(8, 24)[0].abs_diff(first.get_pixel(10, 24)[0]) < 35,
                "{kind:?} must alter the live source texture"
            );
            let second = frame_at(&output, 1.2);
            assert!(
                second.get_pixel(8, 24)[1] > 70 && second.get_pixel(8, 24)[0] < 45,
                "{kind:?} must use each current source frame"
            );
            let yellow = second.get_pixel(48, 4);
            assert!(
                yellow[0] > 180 && yellow[1] > 180 && yellow[2] < 45,
                "timed overlay missing: {yellow:?}"
            );
            let transparent = second.get_pixel(48, 20);
            // Keep the comparison on the same CI/encoding/color path so this checks
            // alpha independently of legacy untagged-source color interpretation.
            let mut transparent_annotations = annotations.clone();
            transparent_annotations[1].image_base64 = annotation_png(image::RgbaImage::new(32, 24));
            let (control_output, _, _, _) = export_video_with_annotations(
                &source,
                &[VideoSegment {
                    start: 0.0,
                    end: 3.0,
                    speed: 1.0,
                }],
                &[],
                &transparent_annotations,
                VideoExportPreset::Original,
            )
            .unwrap();
            let control = frame_at(&control_output, 1.2);
            std::fs::remove_file(control_output).unwrap();
            let control = control.get_pixel(48, 20);
            eprintln!(
                "{kind:?} transparent={transparent:?}, control={control:?}, opaque={yellow:?}"
            );
            assert!(
                transparent[0] < 45 && transparent[1] > 130 && transparent[2] < 45,
                "{kind:?} transparent lower overlay half must retain green source: actual={transparent:?}, control={control:?}, opaque={yellow:?}"
            );
            assert!(
                transparent.0.iter().zip(control.0).all(|(actual, expected)| actual.abs_diff(expected) <= 20),
                "{kind:?} transparent region must match source across all channels: actual={transparent:?}, control={control:?}, opaque={yellow:?}"
            );
            let after = frame_at(&output, 2.7);
            assert!(after.get_pixel(8, 24)[2].abs_diff(after.get_pixel(10, 24)[2]) > 80);
            assert!(
                after.get_pixel(48, 4)[0] < 45,
                "overlay must end independently"
            );
            std::fs::remove_file(output).unwrap();
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_sdr_color_metadata_preserves_small_and_hd_recordings_through_ci() {
        use crate::macos_media::MacosSegmentEncoder;
        for (width, height) in [(64usize, 48usize), (1280, 720)] {
            let directory = tempfile::tempdir().unwrap();
            let source = directory.path().join("color-tagged.mp4");
            let mut encoder = MacosSegmentEncoder::new(
                &source,
                width as u32,
                height as u32,
                30,
                2_000_000,
                false,
            )
            .unwrap();
            let mut frame = vec![0u8; width * height * 4];
            for y in 0..height {
                for x in 0..width {
                    let channel = (x * 3 / width).min(2);
                    frame[(y * width + x) * 4 + channel] = 180;
                    frame[(y * width + x) * 4 + 3] = 255;
                }
            }
            for index in 0..6 {
                assert!(encoder.append_video(&frame, index).unwrap());
            }
            encoder.finish().unwrap();
            let source_bytes = std::fs::read(&source).unwrap();
            let color = source_bytes
                .windows(4)
                .position(|value| value == b"nclx" || value == b"nclc")
                .expect("recording must carry an explicit color description");
            assert_eq!(
                &source_bytes[color + 4..color + 10],
                &[0, 1, 0, 1, 0, 1],
                "recording must tag BT.709 primaries, transfer and YCbCr matrix"
            );
            let annotation = VideoAnnotation {
                layer: -1,
                start: 0.0,
                end: 0.2,
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
                kind: VideoAnnotationKind::Overlay,
                amount: 0.0,
                image_base64: annotation_png(image::RgbaImage::new(1, 1)),
            };
            let (output, _, _, _) = export_video_with_annotations(
                &source,
                &[VideoSegment {
                    start: 0.0,
                    end: 0.2,
                    speed: 1.0,
                }],
                &[],
                &[annotation],
                VideoExportPreset::Original,
            )
            .unwrap();
            let source_png =
                crate::macos_media::video_first_frame_png(&source, width as u32).unwrap();
            let output_png =
                crate::macos_media::video_first_frame_png(&output, width as u32).unwrap();
            let before = image::load_from_memory(&source_png).unwrap().to_rgb8();
            let after = image::load_from_memory(&output_png).unwrap().to_rgb8();
            for part in 0..3 {
                let x = ((part * 2 + 1) * width / 6) as u32;
                let y = (height / 2) as u32;
                let expected = before.get_pixel(x, y);
                let actual = after.get_pixel(x, y);
                assert!(expected.0.iter().zip(actual.0).all(|(expected, actual)| expected.abs_diff(actual) <= 8), "{width}x{height} channel {part}: source={expected:?}, transparent export={actual:?}");
            }
            std::fs::remove_file(output).unwrap();
        }
    }

    #[cfg(target_os = "macos")]
    fn frame_at(source: &Path, time: f64) -> image::RgbImage {
        frame_at_with_edge(source, time, 64)
    }

    #[cfg(target_os = "macos")]
    fn frame_at_with_edge(source: &Path, time: f64, edge: u32) -> image::RgbImage {
        let (sample, _, _, _) = export_video(
            source,
            &[VideoSegment {
                start: time,
                end: time + 0.1,
                speed: 1.0,
            }],
            &[],
            VideoExportPreset::Original,
        )
        .unwrap();
        let bytes = crate::macos_media::video_first_frame_png(&sample, edge).unwrap();
        std::fs::remove_file(sample).unwrap();
        image::load_from_memory(&bytes).unwrap().to_rgb8()
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_removes_middle_and_maps_timed_mask_and_zoom_to_kept_source() {
        use crate::macos_media::MacosSegmentEncoder;
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("colored.mp4");
        let mut encoder = MacosSegmentEncoder::new(&source, 64, 48, 30, 500_000, true).unwrap();
        for index in 0..90 {
            let mut frame = vec![0u8; 64 * 48 * 4];
            for y in 0..48 {
                for x in 0..64 {
                    let color = if index < 30 {
                        [0, 0, 255, 255]
                    } else if index < 60 {
                        [0, 255, 0, 255]
                    } else if x < 32 {
                        [255, 0, 0, 255]
                    } else {
                        [0, 0, 255, 255]
                    };
                    frame[(y * 64 + x) * 4..(y * 64 + x + 1) * 4].copy_from_slice(&color);
                }
            }
            assert!(encoder.append_video(&frame, index).unwrap());
        }
        encoder.append_audio(&vec![0; 3 * 48_000 * 4]).unwrap();
        encoder.finish().unwrap();
        let before = std::fs::read(&source).unwrap();
        let segments = [
            VideoSegment {
                start: 0.0,
                end: 1.0,
                speed: 1.0,
            },
            VideoSegment {
                start: 2.0,
                end: 3.0,
                speed: 1.0,
            },
        ];
        let effects = [
            VideoEffect {
                kind: VideoEffectKind::Zoom,
                start: 2.0,
                end: 2.5,
                x: 0.0,
                y: 0.0,
                width: 0.5,
                height: 0.5,
                ..Default::default()
            },
            VideoEffect {
                kind: VideoEffectKind::Mask,
                start: 2.0,
                end: 2.5,
                x: 0.0,
                y: 0.0,
                width: 0.25,
                height: 0.5,
                ..Default::default()
            },
        ];
        let (output, width, height, duration) =
            export_video(&source, &segments, &effects, VideoExportPreset::Original).unwrap();
        assert_eq!((width, height), (64, 48));
        assert!((duration - 2.0).abs() < 0.05, "{duration}");
        let first = frame_at(&output, 0.2);
        assert!(first.get_pixel(16, 24)[0] > 180);
        let active = frame_at(&output, 1.2);
        assert!(
            active.get_pixel(16, 24).0.iter().all(|value| *value < 35),
            "mask must follow source coordinates through zoom: {:?}",
            active.get_pixel(16, 24)
        );
        assert!(
            active.get_pixel(48, 24)[2] > 180,
            "zoom should show blue left source half, not red right half: {:?}",
            active.get_pixel(48, 24)
        );
        let after = frame_at(&output, 1.7);
        assert!(after.get_pixel(16, 24)[2] > 180);
        assert!(after.get_pixel(48, 24)[0] > 180);
        assert_eq!(before, std::fs::read(&source).unwrap());
        assert_audio_track(&output);
        std::fs::remove_file(output).unwrap();
    }

    #[cfg(target_os = "macos")]
    fn audio_stats(path: &Path, probe_start: f64, probe_end: f64) -> (f64, f64, f64) {
        use std::ffi::{c_char, CString};
        unsafe extern "C" {
            fn kiri_test_video_audio_stats(
                path: *const c_char,
                probe_start: f64,
                probe_end: f64,
                start: *mut f64,
                duration: *mut f64,
                frequency: *mut f64,
            ) -> bool;
        }
        let path = CString::new(path.to_str().unwrap()).unwrap();
        let (mut start, mut duration, mut frequency) = (0.0, 0.0, 0.0);
        assert!(unsafe {
            kiri_test_video_audio_stats(
                path.as_ptr(),
                probe_start,
                probe_end,
                &mut start,
                &mut duration,
                &mut frequency,
            )
        });
        (start, duration, frequency)
    }

    #[cfg(target_os = "macos")]
    fn assert_audio_track(output: &Path) {
        use std::ffi::{c_char, CString};
        unsafe extern "C" {
            fn kiri_macos_has_audio_track(
                path: *const c_char,
                has_audio: *mut bool,
                error: *mut c_char,
                capacity: usize,
            ) -> bool;
        }
        let cpath = CString::new(output.to_str().unwrap()).unwrap();
        let mut has_audio = false;
        let mut error = [0 as c_char; 1024];
        assert!(unsafe {
            kiri_macos_has_audio_track(
                cpath.as_ptr(),
                &mut has_audio,
                error.as_mut_ptr(),
                error.len(),
            )
        });
        assert!(has_audio);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_trim_preserves_source_and_audio_without_upscaling() {
        use crate::macos_media::MacosSegmentEncoder;
        use std::ffi::{c_char, CString};
        unsafe extern "C" {
            fn kiri_macos_has_audio_track(
                path: *const c_char,
                has_audio: *mut bool,
                error: *mut c_char,
                capacity: usize,
            ) -> bool;
        }
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.mp4");
        let mut encoder = MacosSegmentEncoder::new(&source, 64, 48, 30, 500_000, true).unwrap();
        let frame = vec![127; 64 * 48 * 4];
        for index in 0..30 {
            assert!(encoder.append_video(&frame, index).unwrap());
        }
        encoder.append_audio(&vec![0; 48_000 * 4]).unwrap();
        encoder.finish().unwrap();
        let before = std::fs::read(&source).unwrap();
        for preset in [
            VideoExportPreset::Original,
            VideoExportPreset::Share,
            VideoExportPreset::Small,
        ] {
            let (output, width, height, duration) = export_video(
                &source,
                &[VideoSegment {
                    start: 0.2,
                    end: 0.8,
                    speed: 1.0,
                }],
                &[],
                preset,
            )
            .unwrap();
            assert_eq!((width, height), (64, 48));
            assert!((duration - 0.6).abs() < 0.05, "duration: {duration}");
            let cpath = CString::new(output.to_str().unwrap()).unwrap();
            let mut has_audio = false;
            let mut error = [0 as c_char; 1024];
            assert!(unsafe {
                kiri_macos_has_audio_track(
                    cpath.as_ptr(),
                    &mut has_audio,
                    error.as_mut_ptr(),
                    error.len(),
                )
            });
            assert!(has_audio);
            std::fs::remove_file(output).unwrap();
        }
        assert_eq!(before, std::fs::read(source).unwrap());
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn native_effects_respect_rotated_track_display_coordinates() {
        use crate::macos_media::MacosSegmentEncoder;
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("rotated.mp4");
        let mut encoder = MacosSegmentEncoder::new(&source, 64, 48, 30, 500_000, false).unwrap();
        for index in 0..6 {
            assert!(encoder
                .append_video(&vec![127; 64 * 48 * 4], index)
                .unwrap());
        }
        encoder.finish().unwrap();
        // Set the standard ISO BMFF track-header display matrix on this isolated fixture.
        // AVAssetWriter emits version-zero tkhd: matrix starts 48 bytes into the atom.
        let mut bytes = std::fs::read(&source).unwrap();
        let header = bytes.windows(4).position(|value| value == b"tkhd").unwrap() - 4;
        assert_eq!(bytes[header + 8], 0);
        assert_eq!(
            u32::from_be_bytes(bytes[header + 84..header + 88].try_into().unwrap()),
            64 << 16
        );
        let matrix: [i32; 9] = [0, 65536, 0, -65536, 0, 0, 48 * 65536, 0, 1 << 30];
        for (index, value) in matrix.iter().enumerate() {
            bytes[header + 48 + index * 4..header + 52 + index * 4]
                .copy_from_slice(&value.to_be_bytes());
        }
        std::fs::write(&source, bytes).unwrap();
        let (output, width, height, _) = export_video(
            &source,
            &[VideoSegment {
                start: 0.0,
                end: 0.2,
                speed: 1.0,
            }],
            &[VideoEffect {
                kind: VideoEffectKind::Mask,
                start: 0.0,
                end: 0.2,
                x: 0.0,
                y: 0.0,
                width: 0.25,
                height: 0.25,
                ..Default::default()
            }],
            VideoExportPreset::Original,
        )
        .unwrap();
        assert_eq!((width, height), (48, 64));
        let bytes = crate::macos_media::video_first_frame_png(&output, 64).unwrap();
        let image = image::load_from_memory(&bytes).unwrap().to_rgb8();
        assert_eq!(image.dimensions(), (48, 64));
        assert!(image.get_pixel(4, 4).0.iter().all(|value| *value < 35));
        assert!(image.get_pixel(32, 48).0.iter().all(|value| *value > 60));
        std::fs::remove_file(output).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_presets_limit_the_long_edge() {
        use crate::macos_media::MacosSegmentEncoder;
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("wide.mp4");
        let mut encoder =
            MacosSegmentEncoder::new(&source, 1280, 720, 30, 2_000_000, false).unwrap();
        let frame = vec![127; 1280 * 720 * 4];
        for index in 0..6 {
            assert!(encoder.append_video(&frame, index).unwrap());
        }
        encoder.finish().unwrap();
        for (preset, expected) in [
            (VideoExportPreset::Original, (1280, 720)),
            (VideoExportPreset::Share, (1080, 606)),
            (VideoExportPreset::Small, (720, 404)),
        ] {
            let (output, width, height, duration) = export_video(
                &source,
                &[VideoSegment {
                    start: 0.0,
                    end: 0.2,
                    speed: 1.0,
                }],
                &[],
                preset,
            )
            .unwrap();
            assert_eq!((width, height), expected);
            assert!((duration - 0.2).abs() < 0.05);
            std::fs::remove_file(output).unwrap();
        }
        // The Core Image effect path also resizes; test its coordinate system separately.
        let (output, width, height, _) = export_video(
            &source,
            &[VideoSegment {
                start: 0.0,
                end: 0.2,
                speed: 1.0,
            }],
            &[VideoEffect {
                kind: VideoEffectKind::Mask,
                start: 0.0,
                end: 0.2,
                x: 0.0,
                y: 0.0,
                width: 0.25,
                height: 0.5,
                ..Default::default()
            }],
            VideoExportPreset::Small,
        )
        .unwrap();
        assert_eq!((width, height), (720, 404));
        let bytes = crate::macos_media::video_first_frame_png(&output, 64).unwrap();
        let image = image::load_from_memory(&bytes).unwrap().to_rgb8();
        assert!(image.get_pixel(4, 4).0.iter().all(|value| *value < 35));
        assert!(image.get_pixel(48, 24).0.iter().all(|value| *value > 60));
        std::fs::remove_file(output).unwrap();
    }
}
