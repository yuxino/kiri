//! Bounded in-memory library thumbnails (never written to disk).

use std::path::Path;

#[cfg(not(any(target_os = "macos", windows)))]
use std::io::Cursor;

const MAX_THUMBNAIL_EDGE: u32 = 640;
const VIDEO_THUMBNAIL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

#[cfg(target_os = "macos")]
pub fn image_thumbnail(image_path: &Path) -> Option<Vec<u8>> {
    use std::borrow::Borrow;

    use objc2_core_foundation::{
        CFBoolean, CFDictionary, CFMutableData, CFNumber, CFString, CFType, CFURL,
    };
    use objc2_image_io::{
        kCGImageSourceCreateThumbnailFromImageAlways, kCGImageSourceCreateThumbnailWithTransform,
        kCGImageSourceShouldCacheImmediately, kCGImageSourceThumbnailMaxPixelSize,
        CGImageDestination, CGImageSource,
    };

    let url = CFURL::from_file_path(image_path)?;
    let source = unsafe { CGImageSource::with_url(&url, None) }?;
    let max_edge = CFNumber::new_i64(i64::from(MAX_THUMBNAIL_EDGE));
    // ImageIO exports these process-lifetime CFString constants.
    let option_keys = unsafe {
        [
            kCGImageSourceCreateThumbnailFromImageAlways.as_ref(),
            kCGImageSourceCreateThumbnailWithTransform.as_ref(),
            kCGImageSourceShouldCacheImmediately.as_ref(),
            kCGImageSourceThumbnailMaxPixelSize.as_ref(),
        ]
    };
    let options = CFDictionary::<CFType, CFType>::from_slices(
        &option_keys,
        &[
            CFBoolean::new(true).as_ref(),
            CFBoolean::new(true).as_ref(),
            CFBoolean::new(true).as_ref(),
            max_edge.as_ref(),
        ],
    );
    let typed_options: &CFDictionary<CFType, CFType> = &options;
    let opaque_options: &CFDictionary = typed_options.borrow();
    let thumbnail = unsafe { source.thumbnail_at_index(0, Some(opaque_options)) }?;

    let encoded = CFMutableData::new(None, 0)?;
    let png_type = CFString::from_static_str("public.png");
    let destination = unsafe { CGImageDestination::with_data(&encoded, &png_type, 1, None) }?;
    unsafe {
        destination.add_image(&thumbnail, None);
        if !destination.finalize() {
            return None;
        }
    }
    Some(encoded.to_vec())
}

#[cfg(windows)]
pub fn image_thumbnail(image_path: &Path) -> Option<Vec<u8>> {
    use std::os::windows::ffi::OsStrExt;

    use windows::core::{IUnknown, PCWSTR};
    use windows::Win32::Foundation::{GENERIC_READ, HGLOBAL, RPC_E_CHANGED_MODE};
    use windows::Win32::Graphics::Imaging::{
        CLSID_WICImagingFactory, GUID_ContainerFormatPng, GUID_WICPixelFormat32bppBGRA,
        IWICBitmapFrameEncode, IWICImagingFactory, WICBitmapEncoderNoCache,
        WICBitmapInterpolationModeFant, WICDecodeMetadataCacheOnDemand,
    };
    use windows::Win32::System::Com::StructuredStorage::CreateStreamOnHGlobal;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED, STREAM_SEEK_END, STREAM_SEEK_SET,
    };

    struct ComApartment;

    impl Drop for ComApartment {
        fn drop(&mut self) {
            // SAFETY: this guard is created only after a successful
            // CoInitializeEx call on this thread and is dropped on that same
            // thread after all WIC objects.
            unsafe { CoUninitialize() };
        }
    }

    // WIC is a COM API. Protocol requests can arrive on either an initialized
    // WebView thread or a worker thread, so initialize MTA when needed and
    // also accept an existing STA apartment (RPC_E_CHANGED_MODE).
    let initialize_result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let _apartment = if initialize_result.is_ok() {
        Some(ComApartment)
    } else if initialize_result == RPC_E_CHANGED_MODE {
        None
    } else {
        return None;
    };

    let wide_path: Vec<u16> = image_path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    // SAFETY: COM is initialized above, the filename is NUL-terminated for
    // the duration of the decoder call, and every returned interface is owned
    // by a windows-rs smart pointer.
    unsafe {
        let factory: IWICImagingFactory = CoCreateInstance(
            &CLSID_WICImagingFactory,
            None::<&IUnknown>,
            CLSCTX_INPROC_SERVER,
        )
        .ok()?;
        let decoder = factory
            .CreateDecoderFromFilename(
                PCWSTR(wide_path.as_ptr()),
                None,
                GENERIC_READ,
                WICDecodeMetadataCacheOnDemand,
            )
            .ok()?;
        let source = decoder.GetFrame(0).ok()?;
        let mut source_width = 0;
        let mut source_height = 0;
        source.GetSize(&mut source_width, &mut source_height).ok()?;
        let (width, height) =
            thumbnail_dimensions(source_width, source_height, MAX_THUMBNAIL_EDGE)?;

        let scaler = factory.CreateBitmapScaler().ok()?;
        scaler
            .Initialize(&source, width, height, WICBitmapInterpolationModeFant)
            .ok()?;

        // CreateStreamOnHGlobal grows the backing allocation as WIC writes and
        // frees it when the final stream reference is released.
        let stream = CreateStreamOnHGlobal(HGLOBAL::default(), true).ok()?;
        let encoder = factory
            .CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null())
            .ok()?;
        encoder.Initialize(&stream, WICBitmapEncoderNoCache).ok()?;

        let mut frame: Option<IWICBitmapFrameEncode> = None;
        let mut encoder_options = None;
        encoder
            .CreateNewFrame(&mut frame, &mut encoder_options)
            .ok()?;
        let frame = frame?;
        frame.Initialize(encoder_options.as_ref()).ok()?;
        frame.SetSize(width, height).ok()?;
        let mut pixel_format = GUID_WICPixelFormat32bppBGRA;
        frame.SetPixelFormat(&mut pixel_format).ok()?;
        frame.WriteSource(&scaler, std::ptr::null()).ok()?;
        frame.Commit().ok()?;
        encoder.Commit().ok()?;

        let mut encoded_size = 0_u64;
        stream
            .Seek(0, STREAM_SEEK_END, Some(&mut encoded_size))
            .ok()?;
        let encoded_size = u32::try_from(encoded_size).ok()?;
        if encoded_size == 0 {
            return None;
        }
        stream.Seek(0, STREAM_SEEK_SET, None).ok()?;
        let mut encoded = vec![0_u8; encoded_size as usize];
        let mut offset = 0_usize;
        while offset < encoded.len() {
            let mut read = 0_u32;
            let remaining = u32::try_from(encoded.len() - offset).ok()?;
            stream
                .Read(
                    encoded.as_mut_ptr().add(offset).cast(),
                    remaining,
                    Some(&mut read),
                )
                .ok()
                .ok()?;
            if read == 0 {
                return None;
            }
            offset += read as usize;
        }
        Some(encoded)
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn image_thumbnail(image_path: &Path) -> Option<Vec<u8>> {
    let image = image::open(image_path).ok()?;
    let thumbnail = image.thumbnail(MAX_THUMBNAIL_EDGE, MAX_THUMBNAIL_EDGE);
    let mut encoded = Cursor::new(Vec::new());
    thumbnail
        .write_to(&mut encoded, image::ImageFormat::Png)
        .ok()?;
    Some(encoded.into_inner())
}

#[cfg(any(windows, test))]
fn thumbnail_dimensions(width: u32, height: u32, max_edge: u32) -> Option<(u32, u32)> {
    if width == 0 || height == 0 || max_edge == 0 {
        return None;
    }
    if width.max(height) <= max_edge {
        return Some((width, height));
    }

    let (scaled_width, scaled_height) = if width >= height {
        let scaled_height =
            (u64::from(height) * u64::from(max_edge) / u64::from(width)).max(1) as u32;
        (max_edge, scaled_height)
    } else {
        let scaled_width =
            (u64::from(width) * u64::from(max_edge) / u64::from(height)).max(1) as u32;
        (scaled_width, max_edge)
    };
    Some((scaled_width, scaled_height))
}

pub fn video_first_frame(ffmpeg: &Path, video: &Path) -> Option<Vec<u8>> {
    let mut command = std::process::Command::new(ffmpeg);
    command
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-i")
        .arg(video)
        .arg("-frames:v")
        .arg("1")
        .arg("-vf")
        .arg(video_thumbnail_filter())
        .arg("-f")
        .arg("image2pipe")
        .arg("-c:v")
        .arg("png")
        .arg("pipe:1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let output =
        crate::record::run_command_with_timeout(&mut command, VIDEO_THUMBNAIL_TIMEOUT).ok()?;
    if !output.status.success() || output.stdout.is_empty() {
        return None;
    }
    Some(output.stdout)
}

fn video_thumbnail_filter() -> String {
    // Bound both axes before preserving aspect ratio. This caps the long edge
    // for portrait and landscape media and never enlarges a smaller source.
    format!(
        "scale='min({0},iw)':'min({0},ih)':force_original_aspect_ratio=decrease",
        MAX_THUMBNAIL_EDGE
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_thumbnail_preserves_aspect_ratio_and_bounds_dimensions() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("wide.png");
        image::DynamicImage::ImageRgba8(image::RgbaImage::new(1_280, 320))
            .save(&source)
            .unwrap();

        let bytes = image_thumbnail(&source).unwrap();
        let thumbnail =
            image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).unwrap();
        assert_eq!((thumbnail.width(), thumbnail.height()), (640, 160));
    }

    #[test]
    fn thumbnail_dimensions_never_upscale_or_collapse_an_edge() {
        assert_eq!(thumbnail_dimensions(320, 200, 640), Some((320, 200)));
        assert_eq!(thumbnail_dimensions(200, 320, 640), Some((200, 320)));
        assert_eq!(thumbnail_dimensions(1_280, 320, 640), Some((640, 160)));
        assert_eq!(thumbnail_dimensions(320, 1_280, 640), Some((160, 640)));
        assert_eq!(thumbnail_dimensions(1, u32::MAX, 640), Some((1, 640)));
        assert_eq!(thumbnail_dimensions(0, 100, 640), None);
    }

    #[test]
    fn video_thumbnail_filter_bounds_both_axes_without_upscaling() {
        assert_eq!(
            video_thumbnail_filter(),
            "scale='min(640,iw)':'min(640,ih)':force_original_aspect_ratio=decrease"
        );
    }
}
