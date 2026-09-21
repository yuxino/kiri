//! Linux capture backend — Wayland frozen stills.
//!
//! Prefer the system `grim` binary when present (Hyprland/Sway/wlroots). The
//! Screenshot portal is used as a fallback for GNOME/KDE and sandboxed builds
//! that do not ship grim. Hyprland's portal often answers once and then hangs
//! on later silent Screenshot calls, which is why grim is first.
//!
//! Window enumeration is best-effort and may be empty when the compositor does
//! not expose usable bounds.

use std::io::Read;
use std::path::PathBuf;
use std::process::Command;
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

/// Hard upper bound on how long a single portal round-trip may take.
const PORTAL_TIMEOUT: Duration = Duration::from_secs(8);

static FROZEN_CAPTURE_WORKER_ACTIVE: AtomicBool = AtomicBool::new(false);

struct FrozenCaptureWorkerPermit;

impl Drop for FrozenCaptureWorkerPermit {
    fn drop(&mut self) {
        FROZEN_CAPTURE_WORKER_ACTIVE.store(false, Ordering::Release);
    }
}

/// Capture the active display as a frozen PNG.
pub fn capture_active_display() -> Result<CapturedDisplay> {
    if FROZEN_CAPTURE_WORKER_ACTIVE
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        bail!(
            "A previous Linux screen capture is still in progress. Restart Kiri if capture remains unavailable."
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

    match receiver.recv() {
        Ok(result) => result,
        Err(_) => {
            FROZEN_CAPTURE_WORKER_ACTIVE.store(false, Ordering::Release);
            bail!("The Linux screen capture worker stopped unexpectedly.")
        }
    }
}

fn capture_active_display_inner() -> Result<CapturedDisplay> {
    let started = Instant::now();
    let png_bytes = capture_frozen_png()?;
    let image = image::load_from_memory(&png_bytes)
        .map_err(|error| anyhow!("The captured screenshot is not a valid image: {error}"))?;
    let pixel_width = i64::from(image.width());
    let pixel_height = i64::from(image.height());
    if pixel_width <= 0 || pixel_height <= 0 {
        bail!("The captured screenshot had an invalid size.");
    }

    // Final overlay geometry is applied on the GTK thread after we know monitor
    // physical sizes. Until then keep a 1:1 mapping so a wrong GDK_SCALE cannot
    // shrink the overlay.
    let (backing_scale, screen_frame) =
        overlay_geometry_for_capture(pixel_width, pixel_height, &[]);

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
        window_rects: Vec::new(),
        display_id: 1,
        display_identity: None,
        backing_scale,
    })
}

fn capture_frozen_png() -> Result<Vec<u8>> {
    match try_grim_screenshot() {
        Ok(png_bytes) => {
            log::info!(
                "Linux frozen capture: grim succeeded ({} bytes)",
                png_bytes.len()
            );
            return Ok(png_bytes);
        }
        Err(error) => {
            log::info!("Linux frozen capture: grim unavailable ({error}); trying Screenshot portal");
        }
    }
    capture_portal_png()
}

/// Capture via the system `grim` tool (wlroots/Hyprland). Writes PNG to stdout.
/// On Hyprland, prefer the focused output so the PNG matches one display
/// (full-desktop grim spans every monitor and breaks overlay geometry).
fn try_grim_screenshot() -> Result<Vec<u8>> {
    if std::env::var_os("WAYLAND_DISPLAY").is_none()
        && std::env::var_os("WAYLAND_SOCKET").is_none()
    {
        bail!("no Wayland display");
    }
    let mut command = Command::new("grim");
    command.args(["-t", "png"]);
    if let Some(output_name) = hyprland_focused_output() {
        log::info!("Linux frozen capture: grim targeting output {output_name}");
        command.args(["-o", &output_name]);
    }
    command.arg("-");
    let output = command
        .output()
        .map_err(|error| anyhow!("grim could not be started: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "grim exited with {}: {}",
            output.status,
            stderr.trim()
        );
    }
    if output.stdout.is_empty() {
        bail!("grim returned an empty screenshot");
    }
    Ok(output.stdout)
}

fn hyprland_focused_output() -> Option<String> {
    hyprland_focused_monitor().map(|monitor| monitor.name)
}

#[derive(Debug, Clone)]
struct HyprlandMonitor {
    name: String,
    width: f64,
    height: f64,
    scale: f64,
    x: f64,
    y: f64,
}

fn hyprland_focused_monitor() -> Option<HyprlandMonitor> {
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none() {
        return None;
    }
    let output = Command::new("hyprctl")
        .args(["monitors", "-j"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let monitors: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let monitor = monitors.as_array()?.iter().find(|monitor| {
        monitor.get("focused").and_then(|value| value.as_bool()) == Some(true)
    })?;
    Some(HyprlandMonitor {
        name: monitor.get("name")?.as_str()?.to_owned(),
        width: monitor.get("width")?.as_f64().or_else(|| monitor.get("width")?.as_i64().map(|v| v as f64))?,
        height: monitor
            .get("height")?
            .as_f64()
            .or_else(|| monitor.get("height")?.as_i64().map(|v| v as f64))?,
        scale: monitor.get("scale")?.as_f64().unwrap_or(1.0).max(1.0),
        x: monitor
            .get("x")
            .and_then(|value| value.as_f64().or_else(|| value.as_i64().map(|v| v as f64)))
            .unwrap_or(0.0),
        y: monitor
            .get("y")
            .and_then(|value| value.as_f64().or_else(|| value.as_i64().map(|v| v as f64)))
            .unwrap_or(0.0),
    })
}

/// Focused Hyprland output as a monitor hint (physical pixels + compositor scale).
pub(crate) fn hyprland_focused_monitor_hint() -> Option<LinuxMonitorHint> {
    let monitor = hyprland_focused_monitor()?;
    log::info!(
        "Linux Hyprland focused monitor: {} {:.0}x{:.0} at ({:.0},{:.0}) scale={:.2}",
        monitor.name,
        monitor.width,
        monitor.height,
        monitor.x,
        monitor.y,
        monitor.scale
    );
    Some(LinuxMonitorHint {
        pixel_x: monitor.x,
        pixel_y: monitor.y,
        pixel_width: monitor.width,
        pixel_height: monitor.height,
        scale: monitor.scale,
    })
}

fn capture_portal_png() -> Result<Vec<u8>> {
    log::info!("Linux frozen capture: requesting Screenshot portal");

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| anyhow!("Could not start the portal runtime: {error}"))?;

    let uri = runtime
        .block_on(request_portal_screenshot())
        .map_err(|error| anyhow!("{error}"))?;

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
    let _ = std::fs::remove_file(&path);
    Ok(png_bytes)
}

async fn request_portal_screenshot() -> Result<url::Url> {
    match tokio::time::timeout(PORTAL_TIMEOUT, take_screenshot(false)).await {
        Ok(Ok(uri)) => {
            log::info!("Linux frozen capture: silent Screenshot portal succeeded");
            return Ok(uri);
        }
        Ok(Err(error)) => {
            log::info!(
                "Linux frozen capture: silent Screenshot portal refused ({error}); trying interactive"
            );
        }
        Err(_) => {
            bail!(
                "Linux screen capture timed out after {} seconds. \
                 The portal may be busy; try again in a moment.",
                PORTAL_TIMEOUT.as_secs()
            );
        }
    }

    tokio::time::sleep(Duration::from_millis(300)).await;
    match tokio::time::timeout(PORTAL_TIMEOUT, take_screenshot(true)).await {
        Ok(Ok(uri)) => {
            log::info!("Linux frozen capture: interactive Screenshot portal succeeded");
            Ok(uri)
        }
        Ok(Err(error)) => Err(anyhow!("Desktop portal screenshot failed: {error}")),
        Err(_) => Err(anyhow!(
            "Linux screen capture timed out after {} seconds. \
             The portal may be busy; try again in a moment.",
            PORTAL_TIMEOUT.as_secs()
        )),
    }
}

async fn take_screenshot(interactive: bool) -> Result<url::Url, ashpd::Error> {
    let request = Screenshot::request()
        .interactive(interactive)
        .modal(interactive)
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

/// Physical monitor bounds reported by Tauri on the GTK thread.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LinuxMonitorHint {
    pub pixel_x: f64,
    pub pixel_y: f64,
    pub pixel_width: f64,
    pub pixel_height: f64,
    pub scale: f64,
}

pub(crate) fn apply_overlay_geometry(
    display: &mut CapturedDisplay,
    monitors: &[LinuxMonitorHint],
) {
    let (backing_scale, screen_frame) =
        overlay_geometry_for_capture(display.pixel_width, display.pixel_height, monitors);
    display.backing_scale = backing_scale;
    display.screen_frame = screen_frame;
    log::info!(
        "Linux overlay geometry: logical={:.0}x{:.0} at ({:.0},{:.0}) scale={:.2}",
        screen_frame.width,
        screen_frame.height,
        screen_frame.x,
        screen_frame.y,
        backing_scale
    );
}

fn overlay_geometry_for_capture(
    pixel_width: i64,
    pixel_height: i64,
    monitors: &[LinuxMonitorHint],
) -> (f64, Rect) {
    let pixel_width = pixel_width as f64;
    let pixel_height = pixel_height as f64;
    let mut best: Option<(f64, f64, Rect)> = None;

    let consider = |best: &mut Option<(f64, f64, Rect)>,
                    mismatch: f64,
                    scale: f64,
                    logical: Rect| {
        let better = match *best {
            None => true,
            Some((best_mismatch, _, _)) => mismatch < best_mismatch,
        };
        if better {
            *best = Some((mismatch, scale.max(1.0), logical));
        }
    };

    for monitor in monitors {
        let scale = monitor.scale.max(1.0);
        if monitor.pixel_width <= 0.0 || monitor.pixel_height <= 0.0 {
            continue;
        }

        // Tauri/GTK may report size as physical pixels or as logical DIPs.
        let candidates = [
            (
                monitor.pixel_width,
                monitor.pixel_height,
                monitor.pixel_x,
                monitor.pixel_y,
                scale,
            ),
            (
                monitor.pixel_width * scale,
                monitor.pixel_height * scale,
                monitor.pixel_x * scale,
                monitor.pixel_y * scale,
                scale,
            ),
        ];
        for (phys_w, phys_h, phys_x, phys_y, scale) in candidates {
            let mismatch = (phys_w - pixel_width).abs() + (phys_h - pixel_height).abs();
            let logical = Rect::new(
                phys_x / scale,
                phys_y / scale,
                phys_w / scale,
                phys_h / scale,
            );
            let overlay_scale = if logical.width > 0.5 {
                pixel_width / logical.width
            } else {
                scale
            };
            consider(&mut best, mismatch, overlay_scale, logical);
        }
    }

    // Full-desktop grim: match the bounding box of every reported monitor.
    if monitors.len() > 1 {
        for treat_as_logical in [false, true] {
            let mut min_x = f64::INFINITY;
            let mut min_y = f64::INFINITY;
            let mut max_x = f64::NEG_INFINITY;
            let mut max_y = f64::NEG_INFINITY;
            let mut scale = 1.0_f64;
            for monitor in monitors {
                let s = monitor.scale.max(1.0);
                scale = scale.max(s);
                let (w, h, x, y) = if treat_as_logical {
                    (
                        monitor.pixel_width * s,
                        monitor.pixel_height * s,
                        monitor.pixel_x * s,
                        monitor.pixel_y * s,
                    )
                } else {
                    (
                        monitor.pixel_width,
                        monitor.pixel_height,
                        monitor.pixel_x,
                        monitor.pixel_y,
                    )
                };
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x + w);
                max_y = max_y.max(y + h);
            }
            if !min_x.is_finite() {
                continue;
            }
            let phys_w = max_x - min_x;
            let phys_h = max_y - min_y;
            let mismatch = (phys_w - pixel_width).abs() + (phys_h - pixel_height).abs();
            let logical = Rect::new(min_x / scale, min_y / scale, phys_w / scale, phys_h / scale);
            let overlay_scale = if logical.width > 0.5 {
                pixel_width / logical.width
            } else {
                scale
            };
            consider(&mut best, mismatch, overlay_scale, logical);
        }
    }

    match best {
        Some((mismatch, scale, frame)) if mismatch <= 64.0 => (scale, frame),
        _ => {
            // PNG pixels did not match any monitor report (common on Hyprland
            // when GTK/GDK_SCALE and the compositor disagree). Cover the best
            // matching monitor in logical DIPs so the overlay is fullscreen;
            // map the PNG through whatever scale that implies.
            if let Some((scale, frame)) =
                cover_monitor_geometry(pixel_width, pixel_height, monitors)
            {
                (scale, frame)
            } else {
                let scale = inferred_desktop_scale(monitors);
                (
                    scale,
                    Rect::new(0.0, 0.0, pixel_width / scale, pixel_height / scale),
                )
            }
        }
    }
}

/// Pick a monitor to cover when the PNG size disagrees with Tauri's report.
/// Prefer the same aspect ratio and the origin (0,0) display.
fn cover_monitor_geometry(
    pixel_width: f64,
    pixel_height: f64,
    monitors: &[LinuxMonitorHint],
) -> Option<(f64, Rect)> {
    if pixel_width <= 0.0 || pixel_height <= 0.0 || monitors.is_empty() {
        return None;
    }
    let png_aspect = pixel_width / pixel_height;
    let mut best: Option<(f64, f64, Rect)> = None;
    for monitor in monitors {
        let scale = monitor.scale.max(1.0);
        if monitor.pixel_width <= 0.0 || monitor.pixel_height <= 0.0 {
            continue;
        }
        // Interpretation A: Tauri size is physical pixels.
        let logical_a = Rect::new(
            monitor.pixel_x / scale,
            monitor.pixel_y / scale,
            monitor.pixel_width / scale,
            monitor.pixel_height / scale,
        );
        // Interpretation B: Tauri size is already logical DIPs.
        let logical_b = Rect::new(
            monitor.pixel_x,
            monitor.pixel_y,
            monitor.pixel_width,
            monitor.pixel_height,
        );
        for logical in [logical_a, logical_b] {
            if logical.width < 1.0 || logical.height < 1.0 {
                continue;
            }
            let aspect = logical.width / logical.height;
            let aspect_err = (aspect - png_aspect).abs() / png_aspect;
            // Prefer origin monitors and closer aspect ratios.
            let origin_penalty = if logical.x.abs() < 1.0 && logical.y.abs() < 1.0 {
                0.0
            } else {
                0.15
            };
            let score = aspect_err + origin_penalty;
            let overlay_scale = pixel_width / logical.width;
            let better = match best {
                None => true,
                Some((best_score, _, _)) => score < best_score,
            };
            if better {
                best = Some((score, overlay_scale.max(1.0), logical));
            }
        }
    }
    best.map(|(_, scale, frame)| (scale, frame))
}

fn inferred_desktop_scale(monitors: &[LinuxMonitorHint]) -> f64 {
    let from_monitors = monitors
        .iter()
        .map(|monitor| monitor.scale)
        .fold(1.0_f64, f64::max);
    let from_gdk = std::env::var("GDK_SCALE")
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| *value >= 1.0)
        .unwrap_or(1.0);
    from_monitors.max(from_gdk).max(1.0)
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

#[cfg(test)]
mod overlay_geometry_tests {
    use super::{overlay_geometry_for_capture, LinuxMonitorHint};
    use crate::core::geometry::Rect;

    #[test]
    fn uses_scaled_fallback_when_no_monitors_are_listed() {
        let (scale, frame) = overlay_geometry_for_capture(2182, 1272, &[]);
        assert!(scale >= 1.0);
        assert!((frame.width * scale - 2182.0).abs() < 0.01);
        assert!((frame.height * scale - 1272.0).abs() < 0.01);
        assert_eq!(frame.x, 0.0);
        assert_eq!(frame.y, 0.0);
    }

    #[test]
    fn keeps_full_logical_size_when_gdk_scale_would_have_halved_it() {
        let monitors = [LinuxMonitorHint {
            pixel_x: 0.0,
            pixel_y: 0.0,
            pixel_width: 2182.0,
            pixel_height: 1272.0,
            scale: 1.0,
        }];
        let (scale, frame) = overlay_geometry_for_capture(2182, 1272, &monitors);
        assert_eq!(scale, 1.0);
        assert_eq!(frame, Rect::new(0.0, 0.0, 2182.0, 1272.0));
    }

    #[test]
    fn uses_compositor_scale_when_png_matches_physical_pixels() {
        let monitors = [LinuxMonitorHint {
            pixel_x: 0.0,
            pixel_y: 0.0,
            pixel_width: 2182.0,
            pixel_height: 1272.0,
            scale: 2.0,
        }];
        let (scale, frame) = overlay_geometry_for_capture(2182, 1272, &monitors);
        assert_eq!(scale, 2.0);
        assert_eq!(frame, Rect::new(0.0, 0.0, 1091.0, 636.0));
    }

    #[test]
    fn accepts_tauri_size_reported_as_logical_dips() {
        // GTK often reports logical size with scale_factor=2 while grim returns
        // physical pixels.
        let monitors = [LinuxMonitorHint {
            pixel_x: 0.0,
            pixel_y: 0.0,
            pixel_width: 2400.0,
            pixel_height: 720.0,
            scale: 2.0,
        }];
        let (scale, frame) = overlay_geometry_for_capture(4800, 1440, &monitors);
        assert_eq!(scale, 2.0);
        assert_eq!(frame, Rect::new(0.0, 0.0, 2400.0, 720.0));
    }

    #[test]
    fn covers_full_monitor_when_png_and_tauri_sizes_disagree() {
        // Repro from Hyprland: grim=2240x1400, GTK reports 2800x1750@2.
        // Overlay must cover the monitor (1400x875), not PNG/2 (1120x700).
        let monitors = [LinuxMonitorHint {
            pixel_x: 0.0,
            pixel_y: 0.0,
            pixel_width: 2800.0,
            pixel_height: 1750.0,
            scale: 2.0,
        }];
        let (scale, frame) = overlay_geometry_for_capture(2240, 1400, &monitors);
        assert!((frame.width - 1400.0).abs() < 0.01);
        assert!((frame.height - 875.0).abs() < 0.01);
        assert!((scale - 2240.0 / 1400.0).abs() < 0.01);
    }
}
