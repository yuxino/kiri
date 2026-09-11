//! Linux capture backend — xdg-desktop-portal Screenshot for frozen stills.
//!
//! Wayland-first: the portal works on modern GNOME/KDE sessions and on X11
//! sessions that expose the same portal. Window enumeration is best-effort and
//! may be empty when the compositor does not expose usable bounds.

use std::io::Read;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use ashpd::desktop::screenshot::Screenshot;

use crate::core::geometry::Rect;
use crate::record::{AudioChunkSender, AudioSpec};

use super::{CapturedDisplay, PlatformRecorder, VideoFrameSender};

const FROZEN_NATIVE_FRAME_TIMEOUT: Duration = Duration::from_secs(30);
static FROZEN_CAPTURE_WORKER_ACTIVE: AtomicBool = AtomicBool::new(false);

struct FrozenCaptureWorkerPermit;

impl Drop for FrozenCaptureWorkerPermit {
    fn drop(&mut self) {
        FROZEN_CAPTURE_WORKER_ACTIVE.store(false, Ordering::Release);
    }
}

/// Capture the active display as a frozen PNG via the Screenshot portal.
pub fn capture_active_display() -> Result<CapturedDisplay> {
    if FROZEN_CAPTURE_WORKER_ACTIVE
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        bail!(
            "A previous Linux screen capture is still waiting for the desktop portal. Restart Kiri if capture remains unavailable."
        );
    }

    let (sender, receiver) = mpsc::channel();
    let worker = std::thread::Builder::new()
        .name("kiri-frozen-capture".into())
        .spawn(move || {
            let result = {
                let _permit = FrozenCaptureWorkerPermit;
                capture_active_display_inner()
            };
            let _ = sender.send(result);
        });
    if let Err(error) = worker {
        FROZEN_CAPTURE_WORKER_ACTIVE.store(false, Ordering::Release);
        return Err(anyhow!(
            "Could not start the Linux capture worker: {error}"
        ));
    }

    match receiver.recv_timeout(FROZEN_NATIVE_FRAME_TIMEOUT) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => bail!(
            "Linux screen capture did not return a frame within {} seconds. Grant Screen sharing in the system portal dialog if prompted.",
            FROZEN_NATIVE_FRAME_TIMEOUT.as_secs()
        ),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            bail!("The Linux screen capture worker stopped unexpectedly.")
        }
    }
}

fn capture_active_display_inner() -> Result<CapturedDisplay> {
    let started = Instant::now();
    log::info!("Linux frozen capture: requesting Screenshot portal");

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| anyhow!("Could not start the portal runtime: {error}"))?;

    let uri = runtime
        .block_on(request_portal_screenshot())
        .map_err(|error| anyhow!("Desktop portal screenshot failed: {error}"))?;

    let path = portal_uri_to_path(&uri)?;
    let mut file = std::fs::File::open(&path).map_err(|error| {
        anyhow!(
            "The portal screenshot file could not be opened ({}): {error}",
            path.display()
        )
    })?;
    let mut png_bytes = Vec::new();
    file.read_to_end(&mut png_bytes)
        .map_err(|error| anyhow!("The portal screenshot could not be read: {error}"))?;
    // Portal implementations often leave a temporary file; remove best-effort.
    let _ = std::fs::remove_file(&path);

    let image = image::load_from_memory(&png_bytes)
        .map_err(|error| anyhow!("The portal screenshot is not a valid image: {error}"))?;
    let pixel_width = i64::from(image.width());
    let pixel_height = i64::from(image.height());
    if pixel_width <= 0 || pixel_height <= 0 {
        bail!("The portal screenshot had an invalid size.");
    }

    let backing_scale = detect_backing_scale().max(1.0);
    let logical_width = (pixel_width as f64) / backing_scale;
    let logical_height = (pixel_height as f64) / backing_scale;
    let screen_frame = Rect::new(0.0, 0.0, logical_width, logical_height);

    log::info!(
        "Linux frozen capture: {}x{} px (scale {:.2}) in {} ms",
        pixel_width,
        pixel_height,
        backing_scale,
        started.elapsed().as_millis()
    );

    Ok(CapturedDisplay {
        png_data: Arc::<[u8]>::from(png_bytes.into_boxed_slice()),
        pixel_width,
        pixel_height,
        screen_frame,
        // Wayland compositors rarely expose stable window bounds to apps.
        window_rects: Vec::new(),
        display_id: 1,
        display_identity: None,
        backing_scale,
    })
}

async fn request_portal_screenshot() -> Result<url::Url, ashpd::Error> {
    let request = Screenshot::request()
        .interactive(false)
        .modal(true)
        .send()
        .await?;
    let response = request.response()?;
    Ok(response.uri().clone())
}

fn portal_uri_to_path(uri: &url::Url) -> Result<PathBuf> {
    if uri.scheme() != "file" {
        bail!("The portal returned an unsupported screenshot URI scheme ({}).", uri.scheme());
    }
    let path = uri
        .to_file_path()
        .map_err(|_| anyhow!("The portal screenshot URI is not a local file path."))?;
    Ok(path)
}

fn detect_backing_scale() -> f64 {
    // Prefer the compositor-reported scale when GDK exposes it; otherwise keep
    // a 1.0 baseline so logical geometry matches the PNG pixel grid.
    if let Ok(value) = std::env::var("GDK_SCALE") {
        if let Ok(scale) = value.parse::<f64>() {
            if scale.is_finite() && scale >= 1.0 {
                return scale;
            }
        }
    }
    1.0
}

// ---------------------------------------------------------------------------
// Region recorder (PipeWire ScreenCast → bounded BGRA frame queue)
// ---------------------------------------------------------------------------

/// Options mirrored from the shared recording configuration surface.
pub struct LinuxRecorder {
    stop_flag: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<Result<()>>>,
}

impl LinuxRecorder {
    pub fn start(
        region: Rect,
        backing_scale: f64,
        options: crate::core::policy::RecordingOptions,
        video_tx: VideoFrameSender,
        system_audio_tx: Option<AudioChunkSender>,
        microphone_tx: Option<AudioChunkSender>,
    ) -> Result<Self> {
        let _ = (system_audio_tx, microphone_tx);
        let shows_cursor = options.shows_cursor;
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_for_worker = Arc::clone(&stop_flag);
        let worker = std::thread::Builder::new()
            .name("kiri-linux-recorder".into())
            .spawn(move || {
                run_screencast_session(region, backing_scale, shows_cursor, video_tx, stop_for_worker)
            })
            .map_err(|error| anyhow!("Could not start the Linux recorder: {error}"))?;
        Ok(Self {
            stop_flag,
            worker: Some(worker),
        })
    }

    pub fn system_audio_spec(&self) -> Option<AudioSpec> {
        None
    }

    pub fn microphone_spec(&self) -> Option<AudioSpec> {
        None
    }
}

impl PlatformRecorder for LinuxRecorder {
    fn stop(&mut self) -> Result<()> {
        self.stop_flag.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            match worker.join() {
                Ok(result) => result,
                Err(_) => bail!("The Linux recorder worker panicked."),
            }
        } else {
            Ok(())
        }
    }
}

fn run_screencast_session(
    region: Rect,
    backing_scale: f64,
    shows_cursor: bool,
    video_tx: VideoFrameSender,
    stop_flag: Arc<AtomicBool>,
) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| anyhow!("Could not start the ScreenCast runtime: {error}"))?;
    runtime.block_on(async move {
        crate::linux_media::run_pipewire_region_capture(
            region,
            backing_scale,
            shows_cursor,
            video_tx,
            stop_flag,
        )
        .await
    })
}
