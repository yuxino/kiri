//! Import user-selected local media into an isolated normalized snapshot.
use crate::core::asset::CaptureKind;
#[cfg(windows)]
use anyhow::Context;
use anyhow::{bail, Result};
use image::ImageDecoder;
use std::io::Read;
use std::path::Path;

pub struct PreparedMedia {
    pub file: tempfile::NamedTempFile,
    pub kind: CaptureKind,
    pub extension: &'static str,
    pub width: i64,
    pub height: i64,
    pub duration: Option<f64>,
}

pub fn display_title(path: &Path) -> Option<String> {
    let name: String = path.file_stem()?.to_string_lossy().chars()
        .filter(|c| !c.is_control()).take(200).collect();
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_string())
}

pub fn prepare(path: &Path) -> Result<PreparedMedia> {
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_file() {
        bail!("Choose a regular media file");
    }
    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if extension == "mp4" || extension == "mov" {
        if metadata.len() > 8 * 1024 * 1024 * 1024 {
            bail!("Video exceeds 8 GB");
        }
        let mut file = tempfile::Builder::new()
            .suffix(&format!(".{extension}"))
            .tempfile()?;
        let copied = std::io::copy(
            &mut std::fs::File::open(path)?.take(8 * 1024 * 1024 * 1024 + 1),
            file.as_file_mut(),
        )?;
        if copied > 8 * 1024 * 1024 * 1024 {
            bail!("Video exceeds 8 GB");
        }
        let (width, height, duration) = probe_video(file.path())?;
        if width <= 0 || height <= 0 || duration.is_none_or(|d| !d.is_finite() || d <= 0.) {
            bail!("Invalid video");
        }
        // Keep the original container; editing normalizes its exported copy to MP4.
        return Ok(PreparedMedia {
            file,
            kind: CaptureKind::Video,
            extension: if extension == "mov" { "mov" } else { "mp4" },
            width,
            height,
            duration,
        });
    }
    if !["png", "jpg", "jpeg", "webp"].contains(&extension.as_str())
        || metadata.len() > 32 * 1024 * 1024
    {
        bail!("Unsupported image");
    }
    let mut reader = image::ImageReader::open(path)?.with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut image = image::DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    let file = tempfile::Builder::new().suffix(".png").tempfile()?;
    image.save_with_format(file.path(), image::ImageFormat::Png)?;
    Ok(PreparedMedia {
        file,
        kind: CaptureKind::Image,
        extension: "png",
        width: i64::from(image.width()),
        height: i64::from(image.height()),
        duration: None,
    })
}
#[cfg(target_os = "macos")]
fn probe_video(path: &Path) -> Result<(i64, i64, Option<f64>)> {
    crate::macos_media::probe_media(path)
}
#[cfg(windows)]
fn probe_video(path: &Path) -> Result<(i64, i64, Option<f64>)> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        core::HSTRING,
        Storage::StorageFile,
        Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
    };
    unsafe { RoInitialize(RO_INIT_MULTITHREADED) }?;
    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe { RoUninitialize() };
        }
    }
    let _apartment = Apartment;
    let path = std::path::absolute(path)?;
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .map(|unit| if unit == 47 { 92 } else { unit })
        .collect();
    let file = StorageFile::GetFileFromPathAsync(&HSTRING::from_wide(&wide))?.join()?;
    let properties = file.Properties()?.GetVideoPropertiesAsync()?.join()?;
    crate::gif::video_dimensions(&path).context("Video cannot be decoded")?;
    Ok((
        i64::from(properties.Width()?),
        i64::from(properties.Height()?),
        Some(properties.Duration()?.Duration as f64 / 10_000_000.),
    ))
}
#[cfg(not(any(target_os = "macos", windows)))]
fn probe_video(_: &Path) -> Result<(i64, i64, Option<f64>)> {
    bail!("Unsupported platform")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imported_names_are_readable_bounded_and_do_not_include_paths() {
        assert_eq!(display_title(Path::new("/tmp/海边散步.mp4")).as_deref(), Some("海边散步"));
        assert_eq!(display_title(Path::new("/tmp/ hello\n.mp4")).as_deref(), Some("hello"));
        assert_eq!(display_title(Path::new("/tmp/   .png")), None);
        assert_eq!(display_title(Path::new(&format!("{}.mp4", "界".repeat(300)))).unwrap().chars().count(), 200);
    }
    #[test]
    fn images_are_copied_and_normalized_without_changing_source() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("photo.jpg");
        image::RgbImage::from_pixel(31, 19, image::Rgb([210, 30, 25]))
            .save(&path)
            .unwrap();
        let before = std::fs::read(&path).unwrap();
        let imported = prepare(&path).unwrap();
        assert_eq!((imported.width, imported.height), (31, 19));
        assert_eq!(imported.extension, "png");
        assert!(image::open(imported.file.path()).is_ok());
        assert_eq!(before, std::fs::read(path).unwrap());
    }
    #[test]
    fn directories_and_disguised_files_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        assert!(prepare(dir.path()).is_err());
        let path = dir.path().join("bad.png");
        std::fs::write(&path, b"not an image").unwrap();
        assert!(prepare(&path).is_err());
    }
}
