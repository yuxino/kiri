//! GIF export.
//!
//! Windows uses Media Foundation plus the Rust GIF encoder. macOS uses
//! AVFoundation plus ImageIO through the native media bridge.

#[cfg(windows)]
use std::path::{Path, PathBuf};

#[cfg(windows)]
use anyhow::{bail, Context, Result};

#[cfg(any(windows, test))]
pub fn scaled_dimensions(width: u32, height: u32, max_long_edge: u32) -> (u32, u32) {
    if width == 0 || height == 0 || max_long_edge == 0 {
        return (width, height);
    }
    let long_edge = width.max(height);
    if long_edge <= max_long_edge {
        return (width, height);
    }
    let scaled_width =
        ((u64::from(width) * u64::from(max_long_edge)) / u64::from(long_edge)).max(1) as u32;
    let scaled_height =
        ((u64::from(height) * u64::from(max_long_edge)) / u64::from(long_edge)).max(1) as u32;
    (scaled_width, scaled_height)
}

#[cfg(windows)]
fn temporary_gif_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "kiri-gif-{}.gif",
        uuid::Uuid::new_v4().to_string().to_lowercase()
    ))
}

#[cfg(windows)]
struct MediaFoundationGuard;

#[cfg(windows)]
impl MediaFoundationGuard {
    fn start() -> Result<Self> {
        use windows::Win32::Media::MediaFoundation::{MFStartup, MFSTARTUP_FULL, MF_VERSION};
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .context("could not initialize COM for GIF conversion")?;
            if let Err(error) = MFStartup(MF_VERSION, MFSTARTUP_FULL) {
                windows::Win32::System::Com::CoUninitialize();
                return Err(error).context("could not start Windows Media Foundation");
            }
        }
        Ok(Self)
    }
}

#[cfg(windows)]
impl Drop for MediaFoundationGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Media::MediaFoundation::MFShutdown();
            windows::Win32::System::Com::CoUninitialize();
        }
    }
}

#[cfg(windows)]
struct WindowsVideoReader {
    reader: windows::Win32::Media::MediaFoundation::IMFSourceReader,
    width: u32,
    height: u32,
    stride: i32,
    // Fields are dropped in declaration order. Keep Media Foundation alive
    // until after the source reader has released all decoder resources.
    _foundation: MediaFoundationGuard,
}

#[cfg(windows)]
impl WindowsVideoReader {
    fn open(video: &Path) -> Result<Self> {
        use windows::core::HSTRING;
        use windows::Win32::Media::MediaFoundation::{
            MFCreateAttributes, MFCreateMediaType, MFCreateSourceReaderFromURL, MFMediaType_Video,
            MFVideoFormat_RGB32, MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE,
            MF_MT_SUBTYPE, MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, MF_SOURCE_READER_ALL_STREAMS,
            MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, MF_SOURCE_READER_FIRST_VIDEO_STREAM,
        };

        let foundation = MediaFoundationGuard::start()?;
        let mut attributes = None;
        unsafe { MFCreateAttributes(&mut attributes, 2) }
            .context("could not create Media Foundation reader attributes")?;
        let attributes = attributes.context("Media Foundation returned no reader attributes")?;
        unsafe {
            attributes.SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1)?;
            attributes.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1)?;
        }

        let path = HSTRING::from(video.as_os_str().to_os_string());
        let reader = unsafe { MFCreateSourceReaderFromURL(&path, &attributes) }
            .with_context(|| format!("could not open video {}", video.display()))?;
        unsafe {
            reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)?;
            reader.SetStreamSelection(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32, true)?;
        }

        let output_type = unsafe { MFCreateMediaType() }
            .context("could not create the Windows GIF video format")?;
        unsafe {
            output_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            output_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
            reader.SetCurrentMediaType(
                MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                None,
                &output_type,
            )?;
        }

        let current_type =
            unsafe { reader.GetCurrentMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32) }
                .context("could not read the decoded Windows video format")?;
        let frame_size = unsafe { current_type.GetUINT64(&MF_MT_FRAME_SIZE) }
            .context("the decoded Windows video has no frame size")?;
        let width = (frame_size >> 32) as u32;
        let height = frame_size as u32;
        if width == 0 || height == 0 {
            bail!("the decoded Windows video has invalid dimensions");
        }
        let stride = unsafe { current_type.GetUINT32(&MF_MT_DEFAULT_STRIDE) }
            .map(|value| value as i32)
            .unwrap_or_else(|_| (width * 4) as i32);
        if stride == 0 {
            bail!("the decoded Windows video has an invalid row stride");
        }

        Ok(Self {
            reader,
            width,
            height,
            stride,
            _foundation: foundation,
        })
    }

    fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn read_frame(&self) -> Result<Option<(i64, image::RgbaImage)>> {
        use windows::Win32::Media::MediaFoundation::{
            MF_SOURCE_READERF_ENDOFSTREAM, MF_SOURCE_READER_FIRST_VIDEO_STREAM,
        };

        loop {
            let mut flags = 0u32;
            let mut timestamp = 0i64;
            let mut sample = None;
            unsafe {
                self.reader.ReadSample(
                    MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                    0,
                    None,
                    Some(&mut flags),
                    Some(&mut timestamp),
                    Some(&mut sample),
                )?;
            }
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                return Ok(None);
            }
            let Some(sample) = sample else { continue };
            let buffer = unsafe { sample.ConvertToContiguousBuffer() }
                .context("could not read a decoded Windows video frame")?;
            let rgba = self.copy_bgra_frame(&buffer)?;
            return Ok(Some((timestamp, rgba)));
        }
    }

    fn copy_bgra_frame(
        &self,
        buffer: &windows::Win32::Media::MediaFoundation::IMFMediaBuffer,
    ) -> Result<image::RgbaImage> {
        let row_bytes = usize::try_from(self.width)
            .ok()
            .and_then(|width| width.checked_mul(4))
            .context("decoded Windows video row is too wide")?;
        let stride = self.stride.unsigned_abs() as usize;
        if stride < row_bytes {
            bail!("decoded Windows video row stride is too small");
        }
        let required = stride
            .checked_mul(self.height as usize)
            .context("decoded Windows video frame is too large")?;
        let mut pointer = std::ptr::null_mut();
        let mut length = 0u32;
        unsafe { buffer.Lock(&mut pointer, None, Some(&mut length)) }
            .context("could not lock a decoded Windows video frame")?;
        let copy_result = (|| -> Result<image::RgbaImage> {
            if pointer.is_null() || (length as usize) < required {
                bail!(
                    "decoded Windows video frame has {} bytes; expected at least {required}",
                    length
                );
            }
            let bytes = unsafe { std::slice::from_raw_parts(pointer, length as usize) };
            let mut rgba = vec![0u8; row_bytes * self.height as usize];
            for y in 0..self.height as usize {
                // Source Reader's contiguous RGB32 output starts with the
                // display's top row for the normal positive-stride case.
                let source_y = if self.stride < 0 {
                    self.height as usize - 1 - y
                } else {
                    y
                };
                let source = &bytes[source_y * stride..source_y * stride + row_bytes];
                let target = &mut rgba[y * row_bytes..(y + 1) * row_bytes];
                for (bgra, rgba) in source.chunks_exact(4).zip(target.chunks_exact_mut(4)) {
                    rgba.copy_from_slice(&[bgra[2], bgra[1], bgra[0], 255]);
                }
            }
            image::RgbaImage::from_raw(self.width, self.height, rgba)
                .context("could not assemble the decoded Windows video frame")
        })();
        let unlock_result = unsafe { buffer.Unlock() };
        if let Err(error) = unlock_result {
            return Err(error).context("could not unlock a decoded Windows video frame");
        }
        copy_result
    }
}

#[cfg(windows)]
pub fn video_dimensions(video: &Path) -> Result<(u32, u32)> {
    WindowsVideoReader::open(video).map(|reader| reader.dimensions())
}

#[cfg(windows)]
pub fn video_first_frame(video: &Path, max_long_edge: u32) -> Result<Vec<u8>> {
    use image::imageops::FilterType;

    let reader = WindowsVideoReader::open(video)?;
    let (_, mut frame) = reader
        .read_frame()?
        .context("the video did not contain a decodable frame")?;
    let (width, height) = scaled_dimensions(frame.width(), frame.height(), max_long_edge);
    if frame.width() != width || frame.height() != height {
        frame = image::imageops::resize(&frame, width, height, FilterType::Lanczos3);
    }
    let mut cursor = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(frame)
        .write_to(&mut cursor, image::ImageFormat::Png)
        .context("could not encode the video thumbnail")?;
    Ok(cursor.into_inner())
}

/// Exports a Windows MP4 to a looping GIF without FFmpeg.
#[cfg(windows)]
pub fn export_gif(video: &Path, max_long_edge: u32, fps: u32) -> Result<PathBuf> {
    use image::codecs::gif::{GifEncoder, Repeat};
    use image::imageops::FilterType;
    use image::{Delay, Frame};

    if fps == 0 {
        bail!("GIF frame rate must be greater than zero");
    }
    let reader = WindowsVideoReader::open(video)?;
    let (output_width, output_height) =
        scaled_dimensions(reader.width, reader.height, max_long_edge);
    let out_path = temporary_gif_path();
    let result = (|| -> Result<()> {
        let file = std::fs::File::create(&out_path).context("could not create the GIF file")?;
        let mut encoder = GifEncoder::new_with_speed(std::io::BufWriter::new(file), 10);
        encoder
            .set_repeat(Repeat::Infinite)
            .context("could not configure GIF looping")?;
        let delay = Delay::from_numer_denom_ms(1_000, fps);
        let frame_interval = 10_000_000i64 / i64::from(fps);
        let mut next_timestamp = None;
        let mut encoded_frames = 0u64;
        while let Some((timestamp, mut rgba)) = reader.read_frame()? {
            let due = next_timestamp.map(|next| timestamp >= next).unwrap_or(true);
            if !due {
                continue;
            }
            if rgba.width() != output_width || rgba.height() != output_height {
                rgba = image::imageops::resize(
                    &rgba,
                    output_width,
                    output_height,
                    FilterType::Lanczos3,
                );
            }
            encoder
                .encode_frame(Frame::from_parts(rgba, 0, 0, delay))
                .context("could not encode a GIF frame")?;
            encoded_frames = encoded_frames.saturating_add(1);
            let mut next = next_timestamp.unwrap_or(timestamp);
            while next <= timestamp {
                next = next.saturating_add(frame_interval.max(1));
            }
            next_timestamp = Some(next);
        }
        if encoded_frames == 0 {
            bail!("the video did not contain a decodable frame");
        }
        Ok(())
    })();
    if let Err(error) = result {
        let _ = std::fs::remove_file(&out_path);
        return Err(error).context("Kiri could not create the GIF file.");
    }
    Ok(out_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaling_caps_landscape_and_portrait_long_edges() {
        assert_eq!(scaled_dimensions(1920, 1080, 720), (720, 405));
        assert_eq!(scaled_dimensions(1080, 1920, 720), (405, 720));
        assert_eq!(scaled_dimensions(640, 480, 720), (640, 480));
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "requires KIRI_TEST_MP4 to point at a local H.264 MP4 fixture"]
    fn windows_media_foundation_export_smoke() {
        let fixture = std::env::var_os("KIRI_TEST_MP4").expect("KIRI_TEST_MP4 is not set");
        let gif = export_gif(Path::new(&fixture), 720, 12).unwrap();
        let bytes = std::fs::read(&gif).unwrap();
        assert!(bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"));
        println!("native GIF fixture: {}", gif.display());
    }
}
