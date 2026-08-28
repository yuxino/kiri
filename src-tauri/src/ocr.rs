//! Local OCR — Vision on macOS, Windows.Media.Ocr on Windows.
//! Uses accurate recognition with language correction and automatic language
//! detection, returning the top candidate per line.

use anyhow::{anyhow, Result};

#[cfg(target_os = "macos")]
fn make_macos_text_request() -> objc2::rc::Retained<objc2_vision::VNRecognizeTextRequest> {
    use objc2_vision::{VNRecognizeTextRequest, VNRequestTextRecognitionLevel};

    let request = VNRecognizeTextRequest::new();
    request.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
    request.setUsesLanguageCorrection(true);
    request.setAutomaticallyDetectsLanguage(true);
    request
}

#[cfg(target_os = "macos")]
pub fn recognize_text(png: &[u8]) -> Result<String> {
    use objc2::rc::Retained;
    use objc2::AnyThread;
    use objc2_foundation::{NSArray, NSData, NSDictionary};
    use objc2_vision::{VNImageRequestHandler, VNRecognizedTextObservation};

    let data = NSData::with_bytes(png);
    let handler = VNImageRequestHandler::initWithData_options(
        VNImageRequestHandler::alloc(),
        &data,
        &NSDictionary::new(),
    );

    let request = make_macos_text_request();

    let request_ref: &objc2_vision::VNRequest = &request;
    let requests: Retained<NSArray<objc2_vision::VNRequest>> = NSArray::from_slice(&[request_ref]);
    let result = handler.performRequests_error(&requests);
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

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::make_macos_text_request;

    #[test]
    fn macos_request_uses_automatic_language_detection() {
        let request = make_macos_text_request();
        assert!(request.automaticallyDetectsLanguage());
    }
}

#[cfg(windows)]
pub fn recognize_text(png: &[u8]) -> Result<String> {
    use windows::Graphics::Imaging::BitmapPixelFormat;
    use windows::Media::Ocr::OcrEngine;
    use windows::Storage::Streams::DataWriter;

    let image = image::load_from_memory(png).map_err(|error| anyhow!("{error}"))?;
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    let raw = rgba.into_raw();

    // Write the pixel bytes into an IBuffer via DataWriter (the windows crate
    // no longer exposes Buffer::as_mut for raw writes).
    let writer = DataWriter::new().map_err(|error| anyhow!("{error}"))?;
    writer
        .WriteBytes(&raw)
        .map_err(|error| anyhow!("{error}"))?;
    let buffer = writer.DetachBuffer().map_err(|error| anyhow!("{error}"))?;

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
    let result = operation.join().map_err(|error| anyhow!("{error}"))?;

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
