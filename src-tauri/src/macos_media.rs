use std::ffi::{c_char, c_double, c_int, c_longlong, c_uint, c_void, CString};
use std::path::{Path, PathBuf};
use std::ptr::NonNull;

use anyhow::{bail, Context, Result};

const ERROR_CAPACITY: usize = 1024;

unsafe extern "C" {
    fn kiri_macos_encoder_create(
        path: *const c_char,
        width: c_uint,
        height: c_uint,
        fps: c_uint,
        bitrate: c_longlong,
        audio_enabled: bool,
        error: *mut c_char,
        error_capacity: usize,
    ) -> *mut c_void;
    fn kiri_macos_encoder_append_video(
        encoder: *mut c_void,
        bytes: *const u8,
        length: usize,
        frame_index: c_longlong,
        error: *mut c_char,
        error_capacity: usize,
    ) -> c_int;
    fn kiri_macos_encoder_append_audio(
        encoder: *mut c_void,
        bytes: *const u8,
        length: usize,
        error: *mut c_char,
        error_capacity: usize,
    ) -> bool;
    fn kiri_macos_encoder_finish(
        encoder: *mut c_void,
        error: *mut c_char,
        error_capacity: usize,
    ) -> bool;
    fn kiri_macos_encoder_cancel(encoder: *mut c_void);
    fn kiri_macos_encoder_release(encoder: *mut c_void);
    fn kiri_macos_probe_media(
        path: *const c_char,
        width: *mut c_longlong,
        height: *mut c_longlong,
        duration: *mut c_double,
        error: *mut c_char,
        error_capacity: usize,
    ) -> bool;
    #[cfg(test)]
    fn kiri_macos_has_audio_track(
        path: *const c_char,
        has_audio: *mut bool,
        error: *mut c_char,
        error_capacity: usize,
    ) -> bool;
    fn kiri_macos_merge_segments(
        paths: *const *const c_char,
        path_count: usize,
        output_path: *const c_char,
        error: *mut c_char,
        error_capacity: usize,
    ) -> bool;
    fn kiri_macos_export_gif(
        source_path: *const c_char,
        output_path: *const c_char,
        max_long_edge: c_uint,
        fps: c_uint,
        width: *mut c_longlong,
        height: *mut c_longlong,
        duration: *mut c_double,
        error: *mut c_char,
        error_capacity: usize,
    ) -> bool;
    fn kiri_macos_video_first_frame_png(
        source_path: *const c_char,
        output_path: *const c_char,
        max_long_edge: c_uint,
        error: *mut c_char,
        error_capacity: usize,
    ) -> bool;
}

fn c_path(path: &Path) -> Result<CString> {
    CString::new(
        path.to_str()
            .context("macOS media path is not valid UTF-8")?
            .as_bytes(),
    )
    .context("macOS media path contains a null byte")
}

fn error_buffer() -> Vec<c_char> {
    vec![0; ERROR_CAPACITY]
}

fn error_message(buffer: &[c_char], fallback: &str) -> anyhow::Error {
    let bytes = buffer
        .iter()
        .take_while(|byte| **byte != 0)
        .map(|byte| *byte as u8)
        .collect::<Vec<_>>();
    let message = String::from_utf8_lossy(&bytes);
    anyhow::anyhow!(if message.is_empty() {
        fallback.to_string()
    } else {
        message.into_owned()
    })
}

pub struct MacosSegmentEncoder {
    raw: Option<NonNull<c_void>>,
}

unsafe impl Send for MacosSegmentEncoder {}

impl MacosSegmentEncoder {
    pub fn new(
        path: &Path,
        width: u32,
        height: u32,
        fps: u32,
        bitrate: i64,
        audio_enabled: bool,
    ) -> Result<Self> {
        let path = c_path(path)?;
        let mut error = error_buffer();
        let raw = unsafe {
            kiri_macos_encoder_create(
                path.as_ptr(),
                width,
                height,
                fps,
                bitrate,
                audio_enabled,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        let raw = NonNull::new(raw)
            .ok_or_else(|| error_message(&error, "could not initialize AVAssetWriter"))?;
        Ok(Self { raw: Some(raw) })
    }

    pub fn append_video(&mut self, frame: &[u8], frame_index: i64) -> Result<bool> {
        let mut error = error_buffer();
        let status = unsafe {
            kiri_macos_encoder_append_video(
                self.raw.expect("native encoder is available").as_ptr(),
                frame.as_ptr(),
                frame.len(),
                frame_index,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        match status {
            1 => Ok(true),
            0 => Ok(false),
            _ => Err(error_message(
                &error,
                "AVAssetWriter rejected a video frame",
            )),
        }
    }

    pub fn append_audio(&mut self, bytes: &[u8]) -> Result<()> {
        let mut error = error_buffer();
        let success = unsafe {
            kiri_macos_encoder_append_audio(
                self.raw.expect("native encoder is available").as_ptr(),
                bytes.as_ptr(),
                bytes.len(),
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if success {
            Ok(())
        } else {
            Err(error_message(
                &error,
                "AVAssetWriter rejected an audio buffer",
            ))
        }
    }

    pub fn finish(mut self) -> Result<()> {
        let mut error = error_buffer();
        let raw = self.raw.take().expect("native encoder is available");
        let success =
            unsafe { kiri_macos_encoder_finish(raw.as_ptr(), error.as_mut_ptr(), error.len()) };
        unsafe { kiri_macos_encoder_release(raw.as_ptr()) };
        if success {
            Ok(())
        } else {
            Err(error_message(
                &error,
                "AVAssetWriter could not finalize the MP4",
            ))
        }
    }

    pub fn cancel(mut self) {
        let raw = self.raw.take().expect("native encoder is available");
        unsafe {
            kiri_macos_encoder_cancel(raw.as_ptr());
            kiri_macos_encoder_release(raw.as_ptr());
        }
    }
}

impl Drop for MacosSegmentEncoder {
    fn drop(&mut self) {
        if let Some(raw) = self.raw.take() {
            unsafe {
                kiri_macos_encoder_cancel(raw.as_ptr());
                kiri_macos_encoder_release(raw.as_ptr());
            }
        }
    }
}

pub fn probe_media(path: &Path) -> Result<(i64, i64, Option<f64>)> {
    let path = c_path(path)?;
    let mut width = 0;
    let mut height = 0;
    let mut duration = 0.0;
    let mut error = error_buffer();
    let success = unsafe {
        kiri_macos_probe_media(
            path.as_ptr(),
            &mut width,
            &mut height,
            &mut duration,
            error.as_mut_ptr(),
            error.len(),
        )
    };
    if !success {
        return Err(error_message(&error, "AVFoundation could not read the MP4"));
    }
    if width <= 0 || height <= 0 {
        bail!("AVFoundation reported invalid video dimensions");
    }
    Ok((
        width,
        height,
        (duration.is_finite() && duration > 0.0).then_some(duration),
    ))
}

#[cfg(test)]
fn has_audio_track(path: &Path) -> Result<bool> {
    let path = c_path(path)?;
    let mut has_audio = false;
    let mut error = error_buffer();
    let success = unsafe {
        kiri_macos_has_audio_track(
            path.as_ptr(),
            &mut has_audio,
            error.as_mut_ptr(),
            error.len(),
        )
    };
    if success {
        Ok(has_audio)
    } else {
        Err(error_message(
            &error,
            "AVFoundation could not inspect the audio track",
        ))
    }
}

pub fn merge_segments(segments: &[PathBuf], output: &Path) -> Result<()> {
    if segments.is_empty() {
        bail!("No recording segments are available.");
    }
    if segments.len() == 1 {
        std::fs::copy(&segments[0], output)?;
        return Ok(());
    }
    let paths = segments
        .iter()
        .map(|path| c_path(path))
        .collect::<Result<Vec<_>>>()?;
    let pointers = paths.iter().map(|path| path.as_ptr()).collect::<Vec<_>>();
    let output = c_path(output)?;
    let mut error = error_buffer();
    let success = unsafe {
        kiri_macos_merge_segments(
            pointers.as_ptr(),
            pointers.len(),
            output.as_ptr(),
            error.as_mut_ptr(),
            error.len(),
        )
    };
    if success {
        Ok(())
    } else {
        Err(error_message(
            &error,
            "AVFoundation could not merge the recording segments",
        ))
    }
}

pub fn export_gif(
    source: &Path,
    max_long_edge: u32,
    fps: u32,
) -> Result<(PathBuf, i64, i64, Option<f64>)> {
    let output = std::env::temp_dir().join(format!(
        "kiri-gif-{}.gif",
        uuid::Uuid::new_v4().to_string().to_lowercase()
    ));
    let source_c = c_path(source)?;
    let output_c = c_path(&output)?;
    let mut width = 0;
    let mut height = 0;
    let mut duration = 0.0;
    let mut error = error_buffer();
    let success = unsafe {
        kiri_macos_export_gif(
            source_c.as_ptr(),
            output_c.as_ptr(),
            max_long_edge,
            fps,
            &mut width,
            &mut height,
            &mut duration,
            error.as_mut_ptr(),
            error.len(),
        )
    };
    if !success {
        let _ = std::fs::remove_file(&output);
        return Err(error_message(&error, "ImageIO could not create the GIF"));
    }
    Ok((
        output,
        width,
        height,
        (duration.is_finite() && duration > 0.0).then_some(duration),
    ))
}

pub fn video_first_frame_png(source: &Path, max_long_edge: u32) -> Result<Vec<u8>> {
    let output = std::env::temp_dir().join(format!(
        "kiri-video-thumbnail-{}.png",
        uuid::Uuid::new_v4().to_string().to_lowercase()
    ));
    let source = c_path(source)?;
    let output_c = c_path(&output)?;
    let mut error = error_buffer();
    let success = unsafe {
        kiri_macos_video_first_frame_png(
            source.as_ptr(),
            output_c.as_ptr(),
            max_long_edge,
            error.as_mut_ptr(),
            error.len(),
        )
    };
    if !success {
        let _ = std::fs::remove_file(&output);
        return Err(error_message(
            &error,
            "AVFoundation could not create the video thumbnail",
        ));
    }
    let bytes = std::fs::read(&output).context("could not read the native video thumbnail")?;
    let _ = std::fs::remove_file(output);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_media_round_trip_needs_no_external_encoder() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first.mp4");
        let second = directory.path().join("second.mp4");
        for video in [&first, &second] {
            let mut encoder = MacosSegmentEncoder::new(video, 64, 64, 30, 500_000, true).unwrap();
            let mut frame = vec![0_u8; 64 * 64 * 4];
            for (index, pixel) in frame.chunks_exact_mut(4).enumerate() {
                pixel.copy_from_slice(if index / 64 < 32 {
                    &[0, 0, 255, 255]
                } else {
                    &[255, 0, 0, 255]
                });
            }
            for frame_index in 0..3 {
                assert!(encoder.append_video(&frame, frame_index).unwrap());
            }
            let audio = vec![0_u8; 4_800 * 4];
            encoder.append_audio(&audio).unwrap();
            encoder.finish().unwrap();
        }

        let video = directory.path().join("merged.mp4");
        merge_segments(&[first, second], &video).unwrap();

        let (width, height, duration) = probe_media(&video).unwrap();
        assert_eq!((width, height), (64, 64));
        assert!(duration.is_some_and(|seconds| seconds >= 0.19));
        assert!(has_audio_track(&video).unwrap());

        let thumbnail = video_first_frame_png(&video, 64).unwrap();
        let decoded =
            image::load_from_memory_with_format(&thumbnail, image::ImageFormat::Png).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (64, 64));
        let top = decoded.to_rgb8().get_pixel(32, 8).0;
        let bottom = decoded.to_rgb8().get_pixel(32, 56).0;
        assert!(top[0] > top[2], "top half should remain red: {top:?}");
        assert!(
            bottom[2] > bottom[0],
            "bottom half should remain blue: {bottom:?}"
        );

        let (gif, gif_width, gif_height, gif_duration) = export_gif(&video, 32, 12).unwrap();
        assert_eq!((gif_width, gif_height), (32, 32));
        assert!(gif_duration.is_some_and(|seconds| seconds > 0.0));
        let bytes = std::fs::read(&gif).unwrap();
        assert!(bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"));
        let _ = std::fs::remove_file(gif);
    }
}
