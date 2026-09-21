//! Windows native timeline renderer. Coordinates are normalized to the source frame.
use super::{
    validate_annotation_duration, validate_effects, validate_segments, PreparedVideoAnnotation,
    VideoEffect, VideoEffectKind, VideoExportPreset, VideoMaskStyle, VideoSegment,
    ExportProgressRange,
};
use anyhow::{bail, Context, Result};
use std::{collections::HashMap, path::Path};
use windows::{
    core::{HSTRING, Interface, RuntimeType},
    Foundation::{Rect, Size, TimeSpan},
    Media::{
        Editing::{
            BackgroundAudioTrack, MediaClip, MediaComposition, MediaOverlay, MediaOverlayLayer,
            MediaTrimmingPreference,
        },
        Effects::VideoTransformEffectDefinition,
        MediaProperties::MediaEncodingProfile,
        Transcoding::{MediaTranscoder, TranscodeFailureReason},
    },
    Storage::{FileProperties::VideoOrientation, StorageFile},
    Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
};
use windows_future::{
    AsyncStatus, AsyncOperationProgressHandler, AsyncActionProgressHandler, IAsyncInfo,
    IAsyncOperation, IAsyncOperationWithProgress, IAsyncActionWithProgress,
};

#[path = "video_annotation_windows.rs"]
mod annotations;
#[path = "video_speed_windows.rs"]
mod speed;

// WinRT rejects forward slashes even though Rust file APIs accept them.
// Keep UTF-16 intact so paths containing non-ASCII names remain readable.
#[cfg(test)]
fn storage_file(path: &Path) -> Result<StorageFile> {
    storage_file_controlled(path, &super::ExportControl::default().rendering())
}

fn storage_file_controlled(path: &Path, progress: &ExportProgressRange) -> Result<StorageFile> {
    use std::os::windows::ffi::OsStrExt;
    let absolute = std::path::absolute(path)?;
    let wide: Vec<u16> = absolute
        .as_os_str()
        .encode_wide()
        .map(|unit| {
            if unit == b'/' as u16 {
                b'\\' as u16
            } else {
                unit
            }
        })
        .collect();
    wait_operation(StorageFile::GetFileFromPathAsync(&HSTRING::from_wide(&wide))?, progress)
        .context("could not open native video media file")
}

fn wait_async(operation: &IAsyncInfo, progress: &ExportProgressRange) -> Result<()> {
    let started = std::time::Instant::now();
    let mut stopping = false;
    let mut timed_out = false;
    while operation.Status()? == AsyncStatus::Started {
        if !stopping && (progress.check().is_err() || started.elapsed().as_secs() >= 3600) {
            timed_out = started.elapsed().as_secs() >= 3600;
            // A provider may reject Cancel after finishing between Status and
            // Cancel. Always wait for its terminal state before removing output.
            let _ = operation.Cancel();
            stopping = true;
        }
        // Wait for the native operation to release its staging file even after
        // requesting cancellation. Returning early would race temp cleanup.
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    progress.check()?;
    if timed_out { bail!("Windows video export timed out"); }
    Ok(())
}

fn wait_operation<T: RuntimeType + 'static>(operation: IAsyncOperation<T>, progress: &ExportProgressRange) -> Result<T> {
    wait_async(&operation.cast()?, progress)?;
    Ok(operation.GetResults()?)
}

fn wait_render(operation: IAsyncOperationWithProgress<TranscodeFailureReason, f64>, progress: &ExportProgressRange) -> Result<TranscodeFailureReason> {
    let updates = progress.clone();
    operation.SetProgress(&AsyncOperationProgressHandler::new(move |_, percent| {
        updates.report(*percent / 100.0);
        Ok(())
    }))?;
    wait_async(&operation.cast()?, progress)?;
    let result = operation.GetResults()?;
    if result == TranscodeFailureReason::None { progress.report(1.0); }
    Ok(result)
}

fn wait_transcode(operation: IAsyncActionWithProgress<f64>, progress: &ExportProgressRange) -> Result<()> {
    let updates = progress.clone();
    operation.SetProgress(&AsyncActionProgressHandler::new(move |_, percent| {
        updates.report(*percent / 100.0);
        Ok(())
    }))?;
    wait_async(&operation.cast()?, progress)?;
    operation.GetResults()?;
    progress.report(1.0);
    Ok(())
}

fn ticks(seconds: f64) -> i64 {
    (seconds * 10_000_000.0).round() as i64
}

pub(super) fn platform_export(
    source: &Path,
    output: &Path,
    segments: &[VideoSegment],
    effects: &[VideoEffect],
    annotations: &[PreparedVideoAnnotation],
    preset: VideoExportPreset,
    progress: &ExportProgressRange,
) -> Result<(i64, i64, f64)> {
    progress.check()?;
    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe { RoUninitialize() };
        }
    }
    unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.context("could not initialize Windows media")?;
    let _apartment = Apartment;
    let input = storage_file_controlled(source, progress)?;
    let properties = wait_operation(input.Properties()?.GetVideoPropertiesAsync()?, progress)?;
    let source_clip = wait_operation(MediaClip::CreateFromFileAsync(&input)?, progress)?;
    // Use the editor's exact timebase rather than rounded shell metadata duration.
    let duration = source_clip.OriginalDuration()?.Duration as f64 / 10_000_000.0;
    let segments = validate_segments(segments, Some(duration))?;
    validate_effects(effects, Some(duration))?;
    validate_annotation_duration(annotations, duration)?;
    // Reject ambiguous rotated coordinates instead of silently exposing a masked region.
    // Kiri's own capture encoder writes upright frames without rotation metadata.
    let has_speed_changes = segments.iter().any(|segment| segment.speed != 1.0);
    if (!effects.is_empty() || !annotations.is_empty() || has_speed_changes)
        && properties.Orientation()? != VideoOrientation::Normal
    {
        bail!("Windows cannot apply video effects to rotated source videos; export an upright copy first");
    }
    let profile = wait_operation(MediaEncodingProfile::CreateFromFileAsync(&input)?, progress)?;
    if has_speed_changes {
        // Render source-time edits first, in the requested slice order. The speed
        // pass then operates on contiguous master intervals, including reorders.
        let staging = tempfile::Builder::new()
            .prefix("kiri-speed-master-")
            .tempdir()?;
        let master = staging.path().join("ordered-master.mp4");
        let normal_segments: Vec<_> = segments
            .iter()
            .map(|segment| VideoSegment {
                speed: 1.0,
                ..*segment
            })
            .collect();
        platform_export(
            source,
            &master,
            &normal_segments,
            effects,
            annotations,
            preset,
            &progress.child(0.0, 0.5),
        )?;
        let master_profile =
            wait_operation(MediaEncodingProfile::CreateFromFileAsync(&storage_file_controlled(&master, progress)?)?, progress)?;
        speed::render(&master, output, &segments, &master_profile, &progress.child(0.5, 1.0))?;
        let actual = wait_operation(storage_file_controlled(output, progress)?
            .Properties()?.GetVideoPropertiesAsync()?, progress)?;
        return Ok((
            i64::from(actual.Width()?),
            i64::from(actual.Height()?),
            actual.Duration()?.Duration as f64 / 10_000_000.0,
        ));
    }
    let requires_frame_render = !annotations.is_empty()
        || effects.iter().any(|effect| {
            matches!(
                effect.kind,
                VideoEffectKind::Spotlight | VideoEffectKind::Frame | VideoEffectKind::Fade
            ) || (effect.kind == VideoEffectKind::Zoom && effect.transition > 0.0)
                || (effect.kind == VideoEffectKind::Mask
                    && (effect.mask_style != VideoMaskStyle::Solid || effect.color != 0))
        });
    if requires_frame_render {
        let staging = tempfile::Builder::new()
            .prefix("kiri-video-annotations-")
            .tempdir()?;
        let silent_path = staging.path().join("annotated-video.mp4");
        annotations::render(
            source,
            &silent_path,
            source_clip.OriginalDuration()?.Duration,
            annotations,
            effects,
            &profile,
            &progress.child(0.0, 0.5),
        )?;
        let tracks = source_clip.EmbeddedAudioTracks()?;
        let annotated_source = if tracks.Size()? > 0 {
            let composed = MediaComposition::new()?;
            composed
                .Clips()?
                .Append(&wait_operation(MediaClip::CreateFromFileAsync(&storage_file_controlled(&silent_path, progress)?)?, progress)?)?;
            // Preserve every embedded source track, including separate microphone/system tracks.
            for index in 0..tracks.Size()? {
                progress.check()?;
                composed.BackgroundAudioTracks()?.Append(
                    &BackgroundAudioTrack::CreateFromEmbeddedAudioTrack(&tracks.GetAt(index)?)?,
                )?;
            }
            let with_audio = staging.path().join("annotated-with-audio.mp4");
            std::fs::File::create(&with_audio)?;
            let result = wait_render(composed
                .RenderToFileWithProfileAsync(
                    &storage_file_controlled(&with_audio, progress)?,
                    MediaTrimmingPreference::Precise,
                    &profile,
                )?, &progress.child(0.5, 0.7))?;
            if result != TranscodeFailureReason::None {
                bail!("Windows cannot preserve annotated video audio: {result:?}");
            }
            with_audio
        } else {
            silent_path
        };
        return platform_export(&annotated_source, output, &segments, &[], &[], preset,
            &progress.child(if tracks.Size()? > 0 {0.7} else {0.5}, 1.0));
    }
    let video = profile.Video()?;
    let (width, height) = (video.Width()?, video.Height()?);
    if width < 2 || height < 2 {
        bail!("Invalid video dimensions");
    }
    let max_edge = preset.max_edge();
    if max_edge > 0 && width.max(height) > max_edge {
        let scale = max_edge as f64 / width.max(height) as f64;
        video.SetWidth(((width as f64 * scale / 2.0).floor() as u32 * 2).max(2))?;
        video.SetHeight(((height as f64 * scale / 2.0).floor() as u32 * 2).max(2))?;
        video
            .SetBitrate(((video.Bitrate()? as f64 * scale * scale).round() as u32).max(500_000))?;
    }
    let (output_width, output_height) = (video.Width()?, video.Height()?);
    let composition = MediaComposition::new()?;
    let masks = MediaOverlayLayer::new()?;
    let images = tempfile::Builder::new()
        .prefix("kiri-video-masks-")
        .tempdir()?;
    let mut mask_files: HashMap<(u32, u32), StorageFile> = HashMap::new();
    let mut output_time = 0_i64;
    let mut overlay_count = 0_usize;
    let mut zoom_count = 0_usize;
    let zoom_duration: f64 = segments.iter().flat_map(|segment| effects.iter()
        .filter(|effect| effect.kind == VideoEffectKind::Zoom)
        .map(move |effect| (segment.end.min(effect.end) - segment.start.max(effect.start)).max(0.0))).sum();
    let mut zoom_done = 0.0;
    for segment in segments {
        progress.check()?;
        // Each zoom interval becomes a native clip with a constant crop. Mask timing
        // remains independent and uses composition time after deleted footage is removed.
        let mut boundaries = vec![ticks(segment.start), ticks(segment.end)];
        for effect in effects
            .iter()
            .filter(|effect| effect.kind == VideoEffectKind::Zoom)
        {
            for boundary in [ticks(effect.start), ticks(effect.end)] {
                if boundary > ticks(segment.start) && boundary < ticks(segment.end) {
                    boundaries.push(boundary);
                }
            }
        }
        boundaries.sort_unstable();
        boundaries.dedup();
        for range in boundaries.windows(2) {
            progress.check()?;
            let (start, end) = (range[0], range[1]);
            let mut clip = source_clip.Clone()?;
            let original_duration = clip.OriginalDuration()?.Duration;
            if end > original_duration || end <= start {
                bail!("Invalid native video clip duration");
            }
            clip.SetTrimTimeFromStart(TimeSpan { Duration: start })?;
            clip.SetTrimTimeFromEnd(TimeSpan {
                Duration: original_duration - end,
            })?;
            let zoom = effects.iter().find(|effect| {
                effect.kind == VideoEffectKind::Zoom
                    && ticks(effect.start) <= start
                    && ticks(effect.end) > start
            });
            let viewport = zoom
                .map(|effect| (effect.x, effect.y, effect.width, effect.height))
                .unwrap_or((0.0, 0.0, 1.0, 1.0));
            if let Some(zoom) = zoom {
                let transform = VideoTransformEffectDefinition::new()?;
                transform.SetCropRectangle(Rect {
                    X: (zoom.x * width as f64) as f32,
                    Y: (zoom.y * height as f64) as f32,
                    Width: (zoom.width * width as f64) as f32,
                    Height: (zoom.height * height as f64) as f32,
                })?;
                transform.SetOutputSize(Size {
                    Width: output_width as f32,
                    Height: output_height as f32,
                })?;
                // Run the transform in the transcoder, then compose its encoded
                // result. Attaching this transform directly to a composition clip
                // fails at runtime on Windows with MF_E_INVALID_STREAM_STATE.
                zoom_count += 1;
                let path = images.path().join(format!("zoom-{zoom_count}.mp4"));
                std::fs::File::create(&path)?;
                let destination = storage_file_controlled(&path, progress)?;
                let transcoder = MediaTranscoder::new()?;
                transcoder.SetTrimStartTime(TimeSpan { Duration: start })?;
                transcoder.SetTrimStopTime(TimeSpan { Duration: end })?;
                transcoder.AddVideoEffectWithSettings(
                    &transform.ActivatableClassId()?,
                    true,
                    &transform.Properties()?,
                )?;
                let prepared = wait_operation(transcoder
                    .PrepareFileTranscodeAsync(&input, &destination, &profile)?, progress)
                    .context("preparing Windows zoom segment")?;
                if !prepared.CanTranscode()? {
                    bail!(
                        "Windows cannot prepare zoom segment: {:?}",
                        prepared.FailureReason()?
                    );
                }
                let zoom_length = (end - start) as f64 / 10_000_000.0;
                wait_transcode(prepared.TranscodeAsync()?, &progress.child(
                    0.5 * zoom_done / zoom_duration, 0.5 * (zoom_done + zoom_length) / zoom_duration))
                    .context("encoding Windows zoom segment")?;
                zoom_done += zoom_length;
                clip = wait_operation(MediaClip::CreateFromFileAsync(&destination)?, progress)
                    .context("opening encoded Windows zoom segment")?;
                let rendered_duration = clip.OriginalDuration()?.Duration;
                if rendered_duration < end - start - 500_000 {
                    bail!("Windows zoom segment was shorter than requested");
                }
                if rendered_duration > end - start {
                    clip.SetTrimTimeFromEnd(TimeSpan {
                        Duration: rendered_duration - (end - start),
                    })?;
                }
            }
            composition.Clips()?.Append(&clip)?;
            for mask in effects
                .iter()
                .filter(|effect| effect.kind == VideoEffectKind::Mask)
            {
                progress.check()?;
                let mask_start = start.max(ticks(mask.start));
                let mask_end = end.min(ticks(mask.end));
                if mask_end <= mask_start {
                    continue;
                }
                let Some(rect) = mask_rectangle(mask, viewport, output_width, output_height) else {
                    continue;
                };
                overlay_count += 1;
                if overlay_count > 4096 {
                    bail!("Too many overlapping video effects; simplify the timeline");
                }
                let dimensions = (rect.Width as u32, rect.Height as u32);
                let mask_file = if let Some(file) = mask_files.get(&dimensions) {
                    file.clone()
                } else {
                    // Match image and destination aspect ratios, preventing image-fit
                    // letterboxing from leaving any part of the privacy rectangle visible.
                    let path = images
                        .path()
                        .join(format!("{}x{}.png", dimensions.0, dimensions.1));
                    image::RgbaImage::from_pixel(
                        dimensions.0,
                        dimensions.1,
                        image::Rgba([0, 0, 0, 255]),
                    )
                    .save(&path)?;
                    let file = storage_file_controlled(&path, progress)?;
                    mask_files.insert(dimensions, file.clone());
                    file
                };
                let mask_clip = wait_operation(MediaClip::CreateFromImageFileAsync(
                    &mask_file,
                    TimeSpan {
                        Duration: mask_end - mask_start,
                    },
                )?, progress)?;
                let overlay = MediaOverlay::CreateWithPositionAndOpacity(&mask_clip, rect, 1.0)?;
                overlay.SetAudioEnabled(false)?;
                // Delay is absolute from the composition start, even within one layer.
                overlay.SetDelay(TimeSpan {
                    Duration: output_time + mask_start - start,
                })?;
                masks.Overlays()?.Append(&overlay)?;
            }
            output_time += end - start;
        }
    }
    if overlay_count > 0 {
        composition.OverlayLayers()?.Append(&masks)?;
    }
    std::fs::File::create(output)?;
    let destination = storage_file_controlled(output, progress)?;
    let result = wait_render(composition
        .RenderToFileWithProfileAsync(&destination, MediaTrimmingPreference::Precise, &profile)?
        , &progress.child(if zoom_duration > 0.0 {0.5} else {0.0}, 1.0))
        .context("rendering Windows edited composition")?;
    if result != TranscodeFailureReason::None {
        bail!("Windows cannot export this video: {result:?}");
    }
    let actual = wait_operation(destination.Properties()?.GetVideoPropertiesAsync()?, progress)?;
    Ok((
        i64::from(actual.Width()?),
        i64::from(actual.Height()?),
        actual.Duration()?.Duration as f64 / 10_000_000.0,
    ))
}

fn mask_rectangle(
    mask: &VideoEffect,
    viewport: (f64, f64, f64, f64),
    width: u32,
    height: u32,
) -> Option<Rect> {
    let (x, y, w, h) = viewport;
    let left = ((mask.x.max(x) - x) / w).clamp(0.0, 1.0);
    let top = ((mask.y.max(y) - y) / h).clamp(0.0, 1.0);
    let right = (((mask.x + mask.width).min(x + w) - x) / w).clamp(0.0, 1.0);
    let bottom = (((mask.y + mask.height).min(y + h) - y) / h).clamp(0.0, 1.0);
    if right <= left || bottom <= top {
        return None;
    }
    // Outward rounding guarantees at least the requested source area is covered.
    let left = (left * width as f64).floor();
    let top = (top * height as f64).floor();
    let right = (right * width as f64).ceil();
    let bottom = (bottom * height as f64).ceil();
    Some(Rect {
        X: left as f32,
        Y: top as f32,
        Width: (right - left) as f32,
        Height: (bottom - top) as f32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real Windows media round-trip, using only disposable synthetic fixtures.
    /// Runs on the Windows CI runner; a missing encoder or compositor is a failure.
    #[test]
    fn windows_native_video_export_smoke() -> Result<()> {
        use windows::{
            Graphics::Imaging::{
                BitmapAlphaMode, BitmapDecoder, BitmapPixelFormat, BitmapTransform,
                ColorManagementMode, ExifOrientationMode,
            },
            Media::{Editing::VideoFramePrecision, MediaProperties::VideoEncodingQuality},
        };
        struct Apartment;
        impl Drop for Apartment {
            fn drop(&mut self) {
                unsafe { RoUninitialize() };
            }
        }
        unsafe { RoInitialize(RO_INIT_MULTITHREADED) }?;
        let _apartment = Apartment;
        let (directory, _cleanup) = if let Some(root) = std::env::var_os("KIRI_VIDEO_QA_DIR") {
            std::fs::create_dir_all(&root)?;
            let temporary = tempfile::Builder::new()
                .prefix("windows-native-video-")
                .tempdir_in(root)?;
            (temporary.keep(), None)
        } else {
            let temporary = tempfile::Builder::new()
                .prefix("kiri-native-video-test-")
                .tempdir()?;
            (temporary.path().to_path_buf(), Some(temporary))
        };
        // Exercise mixed separators and a Unicode name independently of encoding.
        let path_probe = directory.join("路径-check.txt");
        std::fs::write(&path_probe, "local test")?;
        let mixed_path = path_probe.to_string_lossy().replace('\\', "/");
        storage_file(Path::new(&mixed_path)).context("mixed-separator path regression")?;
        let fixture = MediaComposition::new()?;
        for (index, color) in [[240, 20, 20, 255], [20, 20, 240, 255]]
            .into_iter()
            .enumerate()
        {
            let mut bitmap = image::RgbaImage::from_pixel(320, 180, image::Rgba(color));
            if index == 1 {
                // The marker is only at the inspected output position if zoom applied.
                for y in 54..72 {
                    for x in 96..128 {
                        bitmap.put_pixel(x, y, image::Rgba([20, 240, 20, 255]));
                    }
                }
            }
            for y in 100..160 {
                for x in (20..100).chain(200..280) {
                    if (x / 4 + y / 4) % 2 == 0 {
                        bitmap.put_pixel(x, y, image::Rgba([240, 240, 240, 255]));
                    }
                }
            }
            let path = directory.join(format!("fixture-{index}.png"));
            bitmap.save(&path)?;
            let image = storage_file(&path)?;
            let clip = MediaClip::CreateFromImageFileAsync(
                &image,
                TimeSpan {
                    Duration: ticks(2.0),
                },
            )?
            .join()?;
            fixture.Clips()?.Append(&clip)?;
        }
        // A real sine-wave source verifies that annotation prepass keeps audio.
        let audio_path = directory.join("tone.wav");
        let mut wave = Vec::new();
        let data_length = 48_000_u32 * 4 * 2;
        wave.extend_from_slice(b"RIFF");
        wave.extend_from_slice(&(36 + data_length).to_le_bytes());
        wave.extend_from_slice(b"WAVEfmt ");
        wave.extend_from_slice(&16_u32.to_le_bytes());
        wave.extend_from_slice(&1_u16.to_le_bytes());
        wave.extend_from_slice(&1_u16.to_le_bytes());
        wave.extend_from_slice(&48_000_u32.to_le_bytes());
        wave.extend_from_slice(&96_000_u32.to_le_bytes());
        wave.extend_from_slice(&2_u16.to_le_bytes());
        wave.extend_from_slice(&16_u16.to_le_bytes());
        wave.extend_from_slice(b"data");
        wave.extend_from_slice(&data_length.to_le_bytes());
        for sample in 0..192_000 {
            let amplitude =
                ((sample as f64 * 440.0 * std::f64::consts::TAU / 48_000.0).sin() * 8000.0) as i16;
            wave.extend_from_slice(&amplitude.to_le_bytes());
        }
        std::fs::write(&audio_path, wave)?;
        fixture.BackgroundAudioTracks()?.Append(
            &BackgroundAudioTrack::CreateFromFileAsync(&storage_file(&audio_path)?)?.join()?,
        )?;
        let source_path = directory.join("source.mp4");
        std::fs::File::create(&source_path)?;
        let source = storage_file(&source_path)?;
        let profile = MediaEncodingProfile::CreateMp4(VideoEncodingQuality::HD720p)?;
        let video = profile.Video()?;
        video.SetWidth(320)?;
        video.SetHeight(180)?;
        video.SetBitrate(2_000_000)?;
        video.FrameRate()?.SetNumerator(30)?;
        video.FrameRate()?.SetDenominator(1)?;
        let status = fixture
            .RenderToFileWithProfileAsync(&source, MediaTrimmingPreference::Precise, &profile)?
            .join()?;
        assert_eq!(
            status,
            TranscodeFailureReason::None,
            "synthetic source encoding failed"
        );

        // Exercise both cooperative CPU frame cancellation and the WinRT render
        // operation. The output must be released before the worker returns.
        for (name, effects) in [
            ("native", Vec::new()),
            ("frames", vec![VideoEffect {kind: VideoEffectKind::Mask, start: 0.0, end: 4.0,
                mask_style: VideoMaskStyle::Blur, strength: 1.0, ..Default::default()}]),
        ] {
            use super::super::{ExportControl, ExportControlInner, ExportPhase};
            use std::sync::{Arc, OnceLock, Weak, atomic::{AtomicBool, Ordering}};
            let slot: Arc<OnceLock<Weak<ExportControlInner>>> = Arc::new(OnceLock::new());
            let observed = slot.clone();
            let triggered = Arc::new(AtomicBool::new(false));
            let cancelled = triggered.clone();
            let control = ExportControl::new(move |event| {
                if event.phase == ExportPhase::Rendering && event.progress.is_some_and(|value| value < 1.0) {
                    if let Some(inner) = observed.get().and_then(Weak::upgrade) {
                        cancelled.store(ExportControl(inner).request_cancel(), Ordering::Release);
                    }
                }
            });
            assert!(slot.set(Arc::downgrade(&control.0)).is_ok());
            let cancelled_path = directory.join(format!("cancelled-{name}.mp4"));
            let before = std::fs::read(&source_path)?;
            let started = std::time::Instant::now();
            let result = platform_export(&source_path, &cancelled_path,
                &[VideoSegment { start: 0.0, end: 4.0, speed: 1.0 }], &effects, &[],
                VideoExportPreset::Original, &control.rendering());
            assert!(triggered.load(Ordering::Acquire), "{name} cancellation must reach a live native pass");
            assert!(format!("{:#}", result.unwrap_err()).contains(super::super::EXPORT_CANCELLED));
            assert!(started.elapsed() < std::time::Duration::from_secs(30));
            assert_eq!(std::fs::read(&source_path)?, before);
            if cancelled_path.exists() { std::fs::remove_file(&cancelled_path)?; }
        }

        let output = directory.join("edited.mp4");
        let (width, height, duration) = platform_export(
            &source_path,
            &output,
            &[
                VideoSegment {
                    start: 0.5,
                    end: 1.5,
                    speed: 1.0,
                },
                VideoSegment {
                    start: 2.5,
                    end: 3.5,
                    speed: 1.0,
                },
            ],
            &[
                VideoEffect {
                    kind: VideoEffectKind::Zoom,
                    start: 2.5,
                    end: 3.5,
                    x: 0.25,
                    y: 0.25,
                    width: 0.5,
                    height: 0.5,
                    ..Default::default()
                },
                VideoEffect {
                    kind: VideoEffectKind::Mask,
                    start: 2.75,
                    end: 3.25,
                    x: 0.3,
                    y: 0.3,
                    width: 0.1,
                    height: 0.1,
                    ..Default::default()
                },
            ],
            &[],
            VideoExportPreset::Original,
            &super::super::ExportControl::default().rendering(),
        )?;
        assert_eq!((width, height), (320, 180));
        assert!(
            (duration - 2.0).abs() < 0.1,
            "unexpected edited duration: {duration}"
        );
        assert!(std::fs::metadata(&output)?.len() > 1000);
        let exported = storage_file(&output)?;
        let decoded = MediaComposition::new()?;
        decoded
            .Clips()?
            .Append(&MediaClip::CreateFromFileAsync(&exported)?.join()?)?;
        let pixel = |decoded: &MediaComposition,
                     label: &str,
                     time: f64,
                     x: usize,
                     y: usize|
         -> Result<[u8; 3]> {
            let stream = decoded
                .GetThumbnailAsync(
                    TimeSpan {
                        Duration: ticks(time),
                    },
                    320,
                    180,
                    VideoFramePrecision::NearestFrame,
                )?
                .join()?;
            let decoder = BitmapDecoder::CreateAsync(&stream)?.join()?;
            assert_eq!((decoder.PixelWidth()?, decoder.PixelHeight()?), (320, 180));
            let data = decoder
                .GetPixelDataTransformedAsync(
                    BitmapPixelFormat::Rgba8,
                    BitmapAlphaMode::Ignore,
                    &BitmapTransform::new()?,
                    ExifOrientationMode::IgnoreExifOrientation,
                    ColorManagementMode::DoNotColorManage,
                )?
                .join()?
                .DetachPixelData()?;
            image::RgbaImage::from_raw(320, 180, data.to_vec())
                .context("decoded frame has an unexpected byte count")?
                .save(directory.join(format!("{label}-frame-{time:.2}.png")))?;
            let offset = (y * 320 + x) * 4;
            Ok([data[offset], data[offset + 1], data[offset + 2]])
        };
        let red = pixel(&decoded, "effects", 0.5, 64, 36)?;
        assert!(
            red[0] > 180 && red[1] < 60 && red[2] < 60,
            "first retained segment changed: {red:?}"
        );
        for time in [1.1, 1.9] {
            let green = pixel(&decoded, "effects", time, 64, 36)?;
            assert!(
                green[0] < 60 && green[1] > 180 && green[2] < 60,
                "zoom or mask timing wrong at {time}: {green:?}"
            );
        }
        let black = pixel(&decoded, "effects", 1.5, 64, 36)?;
        assert!(
            black.iter().all(|value| *value < 35),
            "privacy mask missing: {black:?}"
        );
        let blue = pixel(&decoded, "effects", 1.5, 112, 144)?;
        assert!(
            blue[0] < 60 && blue[1] < 60 && blue[2] > 180,
            "mask unexpectedly covers surrounding content: {blue:?}"
        );
        use super::super::VideoAnnotationKind;
        let annotation_output = directory.join("annotations.mp4");
        platform_export(
            &source_path,
            &annotation_output,
            &[
                VideoSegment {
                    start: 0.5,
                    end: 1.5,
                    speed: 1.0,
                },
                VideoSegment {
                    start: 2.5,
                    end: 3.5,
                    speed: 1.0,
                },
            ],
            &[],
            &[
                PreparedVideoAnnotation {
                    layer: -1,
                    start: 0.75,
                    end: 1.25,
                    x: 0.7,
                    y: 0.1,
                    width: 0.1,
                    height: 0.1,
                    kind: VideoAnnotationKind::Overlay,
                    image: image::RgbaImage::from_pixel(32, 18, image::Rgba([240, 240, 20, 255])),
                    amount: 0.0,
                },
                PreparedVideoAnnotation {
                    layer: -1,
                    start: 0.5,
                    end: 3.5,
                    x: 0.625,
                    y: 100.0 / 180.0,
                    width: 0.25,
                    height: 60.0 / 180.0,
                    kind: VideoAnnotationKind::Pixelate,
                    image: image::RgbaImage::from_pixel(80, 60, image::Rgba([0, 0, 0, 255])),
                    amount: 0.125,
                },
                PreparedVideoAnnotation {
                    layer: -1,
                    start: 0.5,
                    end: 3.5,
                    x: 20.0 / 320.0,
                    y: 100.0 / 180.0,
                    width: 0.25,
                    height: 60.0 / 180.0,
                    kind: VideoAnnotationKind::Blur,
                    image: image::RgbaImage::from_pixel(80, 60, image::Rgba([0, 0, 0, 255])),
                    amount: 0.025,
                },
            ],
            VideoExportPreset::Original,
            &super::super::ExportControl::default().rendering(),
        )?;
        let annotated_clip =
            MediaClip::CreateFromFileAsync(&storage_file(&annotation_output)?)?.join()?;
        assert!(
            annotated_clip.EmbeddedAudioTracks()?.Size()? > 0,
            "annotation prepass dropped audio"
        );
        assert!((annotated_clip.OriginalDuration()?.Duration - ticks(2.0)).abs() < ticks(0.1));
        let annotated = MediaComposition::new()?;
        annotated.Clips()?.Append(&annotated_clip)?;
        for (time, dominant) in [(0.5, 0), (1.5, 2)] {
            for x in [60, 220] {
                let mosaic = pixel(&annotated, "annotations", time, x, 120)?;
                assert!(
                    mosaic[dominant] > 180 && mosaic[1] > 60 && mosaic[1] < 200,
                    "live mosaic failed or froze source at {time}, x={x}: {mosaic:?}"
                );
            }
        }
        let yellow = pixel(&annotated, "annotations", 0.5, 240, 27)?;
        assert!(
            yellow[0] > 180 && yellow[1] > 180 && yellow[2] < 60,
            "timed overlay missing: {yellow:?}"
        );
        for time in [0.1, 0.9] {
            let clear = pixel(&annotated, "annotations", time, 240, 27)?;
            assert!(
                clear[0] > 180 && clear[1] < 60 && clear[2] < 60,
                "overlay timing wrong at {time}: {clear:?}"
            );
        }
        let styled_output = directory.join("styled-effects.mp4");
        platform_export(
            &source_path,
            &styled_output,
            &[VideoSegment {
                start: 0.0,
                end: 4.0,
                speed: 1.0,
            }],
            &[
                VideoEffect {
                    kind: VideoEffectKind::Mask,
                    start: 0.25,
                    end: 3.75,
                    x: 20.0 / 320.0,
                    y: 100.0 / 180.0,
                    width: 0.25,
                    height: 60.0 / 180.0,
                    mask_style: VideoMaskStyle::Blur,
                    strength: 1.0,
                    ..Default::default()
                },
                VideoEffect {
                    kind: VideoEffectKind::Mask,
                    start: 0.25,
                    end: 3.75,
                    x: 200.0 / 320.0,
                    y: 100.0 / 180.0,
                    width: 0.25,
                    height: 60.0 / 180.0,
                    mask_style: VideoMaskStyle::Pixelate,
                    strength: 1.0,
                    ..Default::default()
                },
                VideoEffect {
                    kind: VideoEffectKind::Mask,
                    start: 0.25,
                    end: 1.75,
                    x: 0.7,
                    y: 0.1,
                    width: 0.1,
                    height: 0.1,
                    color: 0x20cc80,
                    ..Default::default()
                },
                VideoEffect {
                    kind: VideoEffectKind::Zoom,
                    start: 2.0,
                    end: 4.0,
                    x: 0.25,
                    y: 0.25,
                    width: 0.5,
                    height: 0.5,
                    transition: 0.6,
                    ..Default::default()
                },
            ],
            &[],
            VideoExportPreset::Original,
            &super::super::ExportControl::default().rendering(),
        )?;
        let styled = MediaComposition::new()?;
        let styled_clip = MediaClip::CreateFromFileAsync(&storage_file(&styled_output)?)?.join()?;
        assert!(
            styled_clip.EmbeddedAudioTracks()?.Size()? > 0,
            "styled effects dropped audio"
        );
        assert!((styled_clip.OriginalDuration()?.Duration - ticks(4.0)).abs() < ticks(0.1));
        styled.Clips()?.Append(&styled_clip)?;
        // The native image-clip fixture repeats its red end frame at the 2 s
        // join; subsequent encoding can quantize that join by another frame.
        // Sample inside the blue clip, still within the 0.6 s zoom entrance,
        // so this checks changing mask content rather than join rounding.
        let zoom_entry_sample = 2.15;
        for (time, dominant) in [(0.5, 0), (zoom_entry_sample, 2)] {
            for x in [60, 220] {
                let value = pixel(&styled, "styled", time, x, 120)?;
                assert!(
                    value[dominant] > 180 && value[1] > 60 && value[1] < 200,
                    "styled live mask failed at {time}, x={x}: {value:?}"
                );
            }
        }
        let color = pixel(&styled, "styled", 0.5, 240, 27)?;
        assert!(
            color
                .iter()
                .zip([32_i16, 204, 128])
                .all(|(actual, expected)| (i16::from(*actual) - expected).abs() < 35),
            "mask color wrong: {color:?}"
        );
        let after_color = pixel(&styled, "styled", 1.9, 240, 27)?;
        assert!(
            after_color[0] > 180 && after_color[1] < 60 && after_color[2] < 60,
            "color mask outlived interval"
        );
        for time in [zoom_entry_sample, 3.95] {
            let original_marker = pixel(&styled, "styled", time, 100, 56)?;
            let target_marker = pixel(&styled, "styled", time, 64, 36)?;
            assert!(
                original_marker[1] > 180 && original_marker[0] < 60 && original_marker[2] < 60,
                "zoom entrance/exit is not near full frame at {time}: {original_marker:?}"
            );
            assert!(
                target_marker[2] > 180 && target_marker[1] < 60,
                "zoom snapped to its target at ramp endpoint {time}: {target_marker:?}"
            );
        }
        let halfway = pixel(&styled, "styled", 2.3, 96, 54)?;
        assert!(
            halfway[1] > 180 && halfway[0] < 60 && halfway[2] < 60,
            "zoom intermediate camera position missing: {halfway:?}"
        );
        let held = pixel(&styled, "styled", 2.9, 64, 36)?;
        assert!(
            held[1] > 180 && held[0] < 60 && held[2] < 60,
            "zoom never reached target: {held:?}"
        );
        let speed_output = directory.join("reordered-speeds.mp4");
        let (_, _, speed_duration) = platform_export(
            &source_path,
            &speed_output,
            &[
                VideoSegment {
                    start: 2.3,
                    end: 2.9,
                    speed: 0.25,
                },
                VideoSegment {
                    start: 0.2,
                    end: 0.8,
                    speed: 2.0,
                },
                VideoSegment {
                    start: 1.0,
                    end: 1.8,
                    speed: 4.0,
                },
            ],
            &[VideoEffect {
                kind: VideoEffectKind::Mask,
                start: 2.45,
                end: 2.75,
                x: 0.3,
                y: 0.3,
                width: 0.1,
                height: 0.1,
                ..Default::default()
            }],
            &[],
            VideoExportPreset::Original,
            &super::super::ExportControl::default().rendering(),
        )?;
        assert!(
            (speed_duration - 2.9).abs() < 0.12,
            "mixed-speed duration wrong: {speed_duration}"
        );
        let speed_clip = MediaClip::CreateFromFileAsync(&storage_file(&speed_output)?)?.join()?;
        assert!(
            speed_clip.EmbeddedAudioTracks()?.Size()? > 0,
            "speed export dropped audio"
        );
        let sped = MediaComposition::new()?;
        sped.Clips()?.Append(&speed_clip)?;
        for time in [0.3, 2.1] {
            let green = pixel(&sped, "speeds", time, 100, 56)?;
            assert!(
                green[1] > 180 && green[0] < 60 && green[2] < 60,
                "reordered slow slice or effect timing wrong at {time}: {green:?}"
            );
        }
        let masked = pixel(&sped, "speeds", 1.2, 100, 56)?;
        assert!(
            masked.iter().all(|value| *value < 35),
            "source-time mask failed to slow with its slice: {masked:?}"
        );
        for time in [2.55, 2.8] {
            let red = pixel(&sped, "speeds", time, 100, 56)?;
            assert!(
                red[0] > 180 && red[1] < 60 && red[2] < 60,
                "reordered accelerated slice wrong at {time}: {red:?}"
            );
        }
        // The documented Windows policy changes pitch with speed. A real decoded
        // sine wave proves audio is neither muted nor accidentally left at 1x.
        for (start, end, expected_hz) in
            [(0.9, 1.1, 110.0), (2.51, 2.59, 880.0), (2.77, 2.83, 1760.0)]
        {
            let (rms, frequency) = speed::audio_stats(&speed_output, start, end)?;
            assert!(
                rms > 1000.0,
                "speed audio is silent in {start}..{end}: RMS={rms}"
            );
            assert!((frequency - expected_hz).abs() < expected_hz * 0.15 + 15.0,
                "audio speed differs from video in {start}..{end}: {frequency}Hz vs {expected_hz}Hz");
        }
        Ok(())
    }

    fn mask(x: f64, y: f64, width: f64, height: f64) -> VideoEffect {
        VideoEffect {
            kind: VideoEffectKind::Mask,
            start: 0.0,
            end: 1.0,
            x,
            y,
            width,
            height,
            ..Default::default()
        }
    }

    #[test]
    fn privacy_mask_is_clipped_and_mapped_into_zoom_viewport() {
        let rect = mask_rectangle(
            &mask(0.125, 0.25, 0.375, 0.5),
            (0.25, 0.25, 0.5, 0.5),
            1000,
            500,
        )
        .unwrap();
        assert_eq!(
            (rect.X, rect.Y, rect.Width, rect.Height),
            (0.0, 0.0, 500.0, 500.0)
        );
        assert!(mask_rectangle(
            &mask(0.0, 0.0, 0.125, 0.125),
            (0.25, 0.25, 0.5, 0.5),
            1000,
            500
        )
        .is_none());
    }

    #[test]
    fn privacy_mask_rounding_never_reveals_requested_pixels() {
        let rect = mask_rectangle(
            &mask(0.1234, 0.2345, 0.02, 0.03),
            (0.0, 0.0, 1.0, 1.0),
            100,
            100,
        )
        .unwrap();
        assert!(f64::from(rect.X) <= 12.34);
        assert!(f64::from(rect.Y) <= 23.45);
        assert!(f64::from(rect.X + rect.Width) >= 14.34);
        assert!(f64::from(rect.Y + rect.Height) >= 26.45);
    }
}
