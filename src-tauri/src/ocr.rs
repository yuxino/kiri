//! Local OCR — Vision on macOS, Windows.Media.Ocr on Windows.
//! Mirrors TextRecognizer.swift: .accurate level, language correction,
//! languages [zh-Hans, zh-Hant, en-US, ja-JP], top-1 candidate per line.

use anyhow::{anyhow, Result};

#[cfg(target_os = "macos")]
pub fn recognize_text(png: &[u8]) -> Result<String> {
    use objc2::rc::Retained;
    use objc2::AnyThread;
    use objc2_foundation::{NSArray, NSData, NSDictionary, NSString};
    use objc2_vision::{
        VNImageRequestHandler, VNRecognizeTextRequest, VNRecognizedTextObservation,
        VNRequestTextRecognitionLevel,
    };

    let data = NSData::with_bytes(png);
    let handler = unsafe {
        VNImageRequestHandler::initWithData_options(
            VNImageRequestHandler::alloc(),
            &data,
            &NSDictionary::new(),
        )
    };

    let request = VNRecognizeTextRequest::new();
    request.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
    request.setUsesLanguageCorrection(true);
    let zh_hans = NSString::from_str("zh-Hans");
    let zh_hant = NSString::from_str("zh-Hant");
    let en_us = NSString::from_str("en-US");
    let ja_jp = NSString::from_str("ja-JP");
    let languages = NSArray::from_slice(&[&*zh_hans, &*zh_hant, &*en_us, &*ja_jp]);
    request.setRecognitionLanguages(&languages);

    let request_ref: &objc2_vision::VNRequest = &*request;
    let requests: Retained<NSArray<objc2_vision::VNRequest>> =
        NSArray::from_slice(&[request_ref]);
    let result = unsafe { handler.performRequests_error(&requests) };
    if result.is_err() {
        return Err(anyhow!("Text Recognition Failed"));
    }

    let mut lines = Vec::new();
    if let Some(observations) = request.results() {
        for observation in observations.iter() {
            if let Ok(recognized) = observation.downcast::<VNRecognizedTextObservation>() {
                let candidates = recognized.topCandidates(1);
                if let Some(top) = candidates.firstObject() {
                    lines.push(top.string().to_string());
                }
            }
        }
    }
    let text = lines.join("\n");
    if text.trim().is_empty() {
        Err(anyhow!("No Text Found"))
    } else {
        Ok(text)
    }
}

#[cfg(windows)]
pub fn recognize_text(png: &[u8]) -> Result<String> {
    use windows::core::Interface;
    use windows::Graphics::Imaging::BitmapPixelFormat;
    use windows::Media::Ocr::OcrEngine;
    use windows::Storage::Streams::Buffer;

    let image = image::load_from_memory(png).map_err(|error| anyhow!("{error}"))?;
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    let raw = rgba.into_raw();

    let buffer = Buffer::Create(raw.len() as u32).map_err(|error| anyhow!("{error}"))?;
    unsafe {
        let target = buffer.as_mut().ok_or_else(|| anyhow!("buffer unavailable"))?;
        std::ptr::copy_nonoverlapping(raw.as_ptr(), target.as_mut_ptr() as *mut u8, raw.len());
    }
    buffer
        .SetLength(raw.len() as u32)
        .map_err(|error| anyhow!("{error}"))?;
    let bitmap: windows::Graphics::Imaging::SoftwareBitmap =
        windows::Graphics::Imaging::SoftwareBitmap::CreateCopyFromBuffer(
            &buffer,
            BitmapPixelFormat::Rgba8,
            width as i32,
            height as i32,
        )
        .map_err(|error| anyhow!("{error}"))?;

    let engine =
        OcrEngine::TryCreateFromUserProfileLanguages().map_err(|error| anyhow!("{error}"))?;
    let operation = engine
        .RecognizeAsync(&bitmap)
        .map_err(|error| anyhow!("{error}"))?;
    let result = operation.get().map_err(|error| anyhow!("{error}"))?;

    let mut lines = Vec::new();
    let ocr_lines = result.Lines().map_err(|error| anyhow!("{error}"))?;
    for line in ocr_lines {
        lines.push(line.Text().map_err(|error| anyhow!("{error}"))?.to_string());
    }
    let text = lines.join("\n");
    if text.trim().is_empty() {
        Err(anyhow!("No Text Found"))
    } else {
        Ok(text)
    }
}

#[cfg(windows)]
mod windows_ocr {
    // Windows.Media.Ocr implementation (compiled on Windows builds).
    #[allow(dead_code)]
    pub fn _unused() {}
}
