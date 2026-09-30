//! Linux media uses installed GStreamer libraries. No media helper process is
//! launched or downloaded. Every media pipeline is shut down on every exit path.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use ashpd::desktop::screencast::{CursorMode, Screencast, SourceType};
use ashpd::desktop::PersistMode;
use gstreamer::prelude::*;
use gstreamer_video::prelude::*;

use crate::capture::DisplayIdentity;
use crate::core::geometry::Rect;
use crate::core::policy::RecordingPolicy;
use crate::record::{AudioChunkReceiver, EncoderConfig};

#[path = "linux_media_timing.rs"]
mod timing;
use timing::{queue_has_room, FrameTimeline};

const INPUT_POLL: Duration = Duration::from_millis(25);
const PORTAL_START_TIMEOUT: Duration = Duration::from_secs(120);
const FINALIZE_TIMEOUT: Duration = Duration::from_secs(15);

/// GStreamer elements must reach NULL before their last reference is dropped,
/// including when a decode, push, EOS wait, or output validation fails.
struct PipelineGuard(gstreamer::Pipeline);

impl std::ops::Deref for PipelineGuard {
    type Target = gstreamer::Pipeline;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for PipelineGuard {
    fn drop(&mut self) {
        let _ = self.0.set_state(gstreamer::State::Null);
    }
}

fn pipeline(description: &str, operation: &str) -> Result<PipelineGuard> {
    gstreamer::init().context("GStreamer initialization failed")?;
    let element = gstreamer::parse::launch(description).with_context(|| {
        format!("Could not prepare {operation}; check the installed GStreamer plugins")
    })?;
    Ok(PipelineGuard(
        element
            .downcast::<gstreamer::Pipeline>()
            .map_err(|_| anyhow!("GStreamer did not return a pipeline for {operation}."))?,
    ))
}

fn set_file(pipeline: &gstreamer::Pipeline, element: &str, path: &Path) -> Result<()> {
    let path = path
        .to_str()
        .ok_or_else(|| anyhow!("The media path is not valid UTF-8."))?;
    pipeline
        .by_name(element)
        .ok_or_else(|| anyhow!("The media pipeline is missing {element}."))?
        .set_property("location", path);
    Ok(())
}

fn pipeline_bus(pipeline: &gstreamer::Pipeline) -> Result<gstreamer::Bus> {
    pipeline
        .bus()
        .ok_or_else(|| anyhow!("The media pipeline bus is missing."))
}

fn message_failure(message: &gstreamer::MessageRef, operation: &str) -> anyhow::Error {
    match message.view() {
        gstreamer::MessageView::Error(error) => anyhow!(
            "{operation} failed: {} ({})",
            error.error(),
            error.debug().as_deref().unwrap_or("no additional details")
        ),
        _ => anyhow!("{operation} ended unexpectedly."),
    }
}

fn check_pipeline(bus: &gstreamer::Bus, operation: &str) -> Result<()> {
    if let Some(message) =
        bus.pop_filtered(&[gstreamer::MessageType::Error, gstreamer::MessageType::Eos])
    {
        return Err(message_failure(&message, operation));
    }
    Ok(())
}

fn wait_for_eos(bus: &gstreamer::Bus, timeout: Duration, operation: &str) -> Result<()> {
    let message = bus
        .timed_pop_filtered(
            clock_time(timeout),
            &[gstreamer::MessageType::Eos, gstreamer::MessageType::Error],
        )
        .ok_or_else(|| anyhow!("{operation} timed out before finalizing its output."))?;
    match message.view() {
        gstreamer::MessageView::Eos(_) => Ok(()),
        _ => Err(message_failure(&message, operation)),
    }
}

fn clock_time(duration: Duration) -> gstreamer::ClockTime {
    gstreamer::ClockTime::from_nseconds(duration.as_nanos().min(u128::from(u64::MAX - 1)) as u64)
}

fn staged_output(out_path: &Path) -> Result<tempfile::NamedTempFile> {
    tempfile::Builder::new()
        .prefix(".kiri-media-")
        .suffix(".mp4")
        .tempfile_in(out_path.parent().unwrap_or_else(|| Path::new(".")))
        .context("Could not create a temporary media output")
}

// ---------------------------------------------------------------------------
// ScreenCast / X11 → bounded BGRA frames
// ---------------------------------------------------------------------------

/// Keep the portal runtime alive for the whole stream, but never block it while
/// pumping frames. Each permission request is cancellable and shares one
/// deadline; stopping a recording cannot wait indefinitely for the chooser.
pub async fn run_pipewire_region_capture(
    display: DisplayIdentity,
    region: Rect,
    backing_scale: f64,
    shows_cursor: bool,
    video_tx: crate::capture::VideoFrameSender,
    stop_flag: Arc<AtomicBool>,
) -> Result<()> {
    if !crate::platform::linux::is_wayland_session() {
        return tokio::task::spawn_blocking(move || {
            run_x11_region_capture(
                display,
                region,
                backing_scale,
                shows_cursor,
                video_tx,
                stop_flag,
            )
        })
        .await
        .context("The X11 capture worker panicked")?;
    }

    let deadline = tokio::time::Instant::now() + PORTAL_START_TIMEOUT;
    let proxy = portal_step(
        &stop_flag,
        deadline,
        "connect to ScreenCast",
        Screencast::new(),
    )
    .await?;
    let session = portal_step(
        &stop_flag,
        deadline,
        "create ScreenCast session",
        proxy.create_session(),
    )
    .await?;
    let result = async {
        let cursor_mode = if shows_cursor {
            CursorMode::Embedded
        } else {
            CursorMode::Hidden
        };
        portal_step(
            &stop_flag,
            deadline,
            "select ScreenCast sources",
            proxy.select_sources(
                &session,
                cursor_mode,
                SourceType::Monitor.into(),
                false,
                None,
                PersistMode::DoNot,
            ),
        )
        .await?
        .response()
        .context("ScreenCast source selection was cancelled or denied")?;
        let response = portal_step(
            &stop_flag,
            deadline,
            "authorize ScreenCast",
            proxy.start(&session, None),
        )
        .await?
        .response()
        .context("ScreenCast was cancelled or denied")?;
        if response.streams().len() != 1 {
            bail!("Select exactly one display for Linux recording.");
        }
        let stream = &response.streams()[0];
        // Portal sizes/positions are compositor coordinates, not necessarily
        // physical pixels. The first PipeWire frame also validates pixel size.
        validate_portal_display(&display, stream.position(), stream.size())?;
        let node_id = stream.pipe_wire_node_id();
        let fd = portal_step(
            &stop_flag,
            deadline,
            "open the PipeWire stream",
            proxy.open_pipe_wire_remote(&session),
        )
        .await?;
        let stop_for_pump = Arc::clone(&stop_flag);
        tokio::task::spawn_blocking(move || {
            use std::os::fd::AsRawFd;
            let raw_fd = fd.as_raw_fd();
            let pipeline = pipeline(
                &format!(
                    "pipewiresrc fd={raw_fd} path={node_id} do-timestamp=true ! \
                 videoconvert ! video/x-raw,format=BGRA ! \
                 appsink name=sink max-buffers=2 drop=true sync=false"
                ),
                "PipeWire capture",
            )?;
            let crop = capture_crop(
                region,
                backing_scale,
                display.physical_width,
                display.physical_height,
            )?;
            let result = pump_capture_frames(
                pipeline,
                (display.physical_width, display.physical_height),
                crop,
                video_tx,
                stop_for_pump,
            );
            // The authorized remote must outlive every PipeWire element.
            drop(fd);
            result
        })
        .await
        .context("The PipeWire capture worker panicked")?
    }
    .await;
    let _ = tokio::time::timeout(Duration::from_secs(2), session.close()).await;
    if stop_flag.load(Ordering::Acquire) {
        Ok(())
    } else {
        result
    }
}

async fn portal_step<T, E: std::fmt::Display>(
    stop: &AtomicBool,
    deadline: tokio::time::Instant,
    operation: &str,
    future: impl Future<Output = std::result::Result<T, E>>,
) -> Result<T> {
    tokio::select! {
        result = tokio::time::timeout_at(deadline, future) => {
            result
                .map_err(|_| anyhow!("Linux ScreenCast authorization timed out."))?
                .map_err(|error| anyhow!("Could not {operation}: {error}"))
        }
        _ = async {
            while !stop.load(Ordering::Acquire) {
                tokio::time::sleep(INPUT_POLL).await;
            }
        } => bail!("Linux ScreenCast was cancelled."),
    }
}

fn validate_portal_display(
    display: &DisplayIdentity,
    position: Option<(i32, i32)>,
    size: Option<(i32, i32)>,
) -> Result<()> {
    let scale = display.scale_factor;
    if !scale.is_finite() || scale <= 0.0 {
        bail!("The selected display has an invalid scale.");
    }
    let matches = |actual: i32, expected: f64| (f64::from(actual) - expected).abs() <= 1.0;
    if size.is_some_and(|(width, height)| {
        !matches(width, f64::from(display.physical_width) / scale)
            || !matches(height, f64::from(display.physical_height) / scale)
    }) || position.is_some_and(|(x, y)| {
        !matches(x, f64::from(display.physical_x) / scale)
            || !matches(y, f64::from(display.physical_y) / scale)
    }) {
        bail!("The ScreenCast display does not match the screenshot. Select the same display and try again.");
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct PixelCrop {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

fn capture_crop(region: Rect, scale: f64, full_width: u32, full_height: u32) -> Result<PixelCrop> {
    if ![region.x, region.y, region.width, region.height, scale]
        .iter()
        .all(|value| value.is_finite())
        || scale <= 0.0
        || region.x < 0.0
        || region.y < 0.0
        || region.width <= 0.0
        || region.height <= 0.0
    {
        bail!("The Linux recording region is invalid.");
    }
    let crop = PixelCrop {
        x: (region.x * scale).round() as u32,
        y: (region.y * scale).round() as u32,
        width: u32::try_from(RecordingPolicy::pixel_dimension(region.width, scale))?,
        height: u32::try_from(RecordingPolicy::pixel_dimension(region.height, scale))?,
    };
    if crop.width == 0
        || crop.height == 0
        || crop
            .x
            .checked_add(crop.width)
            .is_none_or(|end| end > full_width)
        || crop
            .y
            .checked_add(crop.height)
            .is_none_or(|end| end > full_height)
    {
        bail!("The recording region no longer fits the selected display.");
    }
    Ok(crop)
}

fn run_x11_region_capture(
    display: DisplayIdentity,
    region: Rect,
    backing_scale: f64,
    shows_cursor: bool,
    video_tx: crate::capture::VideoFrameSender,
    stop_flag: Arc<AtomicBool>,
) -> Result<()> {
    let crop = capture_crop(
        region,
        backing_scale,
        display.physical_width,
        display.physical_height,
    )?;
    let x = i64::from(display.physical_x) + i64::from(crop.x);
    let y = i64::from(display.physical_y) + i64::from(crop.y);
    if x < 0 || y < 0 {
        bail!("The X11 display has unsupported root-window coordinates.");
    }
    let end_x = x + i64::from(crop.width) - 1;
    let end_y = y + i64::from(crop.height) - 1;
    let fps = RecordingPolicy::FRAMES_PER_SECOND;
    let pipeline = pipeline(&format!(
        "ximagesrc use-damage=false show-pointer={shows_cursor} startx={x} starty={y} endx={end_x} endy={end_y} ! \
         video/x-raw,framerate={fps}/1 ! videoconvert ! video/x-raw,format=BGRA ! \
         appsink name=sink max-buffers=2 drop=true sync=false"
    ), "X11 capture")?;
    pump_capture_frames(
        pipeline,
        (crop.width, crop.height),
        PixelCrop { x: 0, y: 0, ..crop },
        video_tx,
        stop_flag,
    )
}

fn pump_capture_frames(
    pipeline: PipelineGuard,
    expected: (u32, u32),
    crop: PixelCrop,
    video_tx: crate::capture::VideoFrameSender,
    stop: Arc<AtomicBool>,
) -> Result<()> {
    let sink = pipeline
        .by_name("sink")
        .ok_or_else(|| anyhow!("The capture appsink is missing."))?
        .downcast::<gstreamer_app::AppSink>()
        .map_err(|_| anyhow!("The capture sink has the wrong type."))?;
    let bus = pipeline_bus(&pipeline)?;
    pipeline
        .set_state(gstreamer::State::Playing)
        .context("Could not start Linux screen capture")?;
    let mut first_frame_at = None::<Instant>;
    let mut frame_schedule = FrameTimeline::new(RecordingPolicy::FRAMES_PER_SECOND);
    let first_frame_deadline = Instant::now() + Duration::from_secs(15);
    let mut dropped = 0u64;
    while !stop.load(Ordering::Acquire) {
        check_pipeline(&bus, "Linux screen capture")?;
        let Some(sample) = sink.try_pull_sample(clock_time(INPUT_POLL)) else {
            if sink.is_eos() {
                bail!("The screen sharing session ended unexpectedly.");
            }
            if first_frame_at.is_none() && Instant::now() >= first_frame_deadline {
                bail!("The screen sharing session did not deliver a video frame.");
            }
            continue;
        };
        let info = gstreamer_video::VideoInfo::from_caps(
            sample
                .caps()
                .ok_or_else(|| anyhow!("The capture frame has no format."))?,
        )?;
        if (info.width(), info.height()) != expected {
            bail!("The shared display size changed or differs from the screenshot; recording was stopped to avoid capturing the wrong region.");
        }
        let now = Instant::now();
        if first_frame_at
            .is_some_and(|origin| frame_schedule.advance(now.duration_since(origin)).is_none())
        {
            continue;
        }
        let pixels = packed_sample_pixels(&sample, crop)?;
        match video_tx.try_send(pixels) {
            Ok(()) => {
                first_frame_at.get_or_insert(now);
            }
            Err(mpsc::TrySendError::Full(_)) => {
                dropped += 1;
                if dropped == 1 || dropped.is_multiple_of(120) {
                    log::warn!("Linux capture dropped {dropped} frames while the bounded encoder queue was full");
                }
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                bail!("The Linux video encoder stopped accepting frames.")
            }
        }
    }
    Ok(())
}

/// VideoMeta may specify a padded stride that cannot be inferred from total
/// buffer length. Map its packed RGBA/BGRA plane and copy only selected rows.
fn packed_sample_pixels(sample: &gstreamer::Sample, crop: PixelCrop) -> Result<Vec<u8>> {
    let caps = sample
        .caps()
        .ok_or_else(|| anyhow!("The video frame has no format."))?;
    let info = gstreamer_video::VideoInfo::from_caps(caps)?;
    if !matches!(
        info.format(),
        gstreamer_video::VideoFormat::Bgra | gstreamer_video::VideoFormat::Rgba
    ) {
        bail!("The video frame is not packed RGBA or BGRA.");
    }
    let buffer = sample
        .buffer()
        .ok_or_else(|| anyhow!("The video frame has no buffer."))?;
    let frame = gstreamer_video::VideoFrameRef::from_buffer_ref_readable(buffer, &info)?;
    let stride = usize::try_from(frame.plane_stride()[0])
        .context("The video frame has a negative stride")?;
    crop_bgra_frame(
        frame.plane_data(0)?,
        stride,
        info.width(),
        info.height(),
        crop,
    )
    .ok_or_else(|| anyhow!("The video frame does not contain the selected region."))
}

fn crop_bgra_frame(
    source: &[u8],
    stride: usize,
    full_width: u32,
    full_height: u32,
    crop: PixelCrop,
) -> Option<Vec<u8>> {
    if crop.width == 0
        || crop.height == 0
        || crop.x.checked_add(crop.width)? > full_width
        || crop.y.checked_add(crop.height)? > full_height
        || stride < (full_width as usize).checked_mul(4)?
    {
        return None;
    }
    let row_bytes = (crop.width as usize).checked_mul(4)?;
    let mut out = vec![0u8; row_bytes.checked_mul(crop.height as usize)?];
    for row in 0..crop.height as usize {
        let offset = (crop.y as usize + row)
            .checked_mul(stride)?
            .checked_add((crop.x as usize).checked_mul(4)?)?;
        out[row * row_bytes..(row + 1) * row_bytes]
            .copy_from_slice(source.get(offset..offset.checked_add(row_bytes)?)?);
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// GStreamer H.264 → MP4 segment encoder (Linux audio is explicitly unavailable)
// ---------------------------------------------------------------------------

pub struct LinuxNativeSegmentEncoder {
    out_path: PathBuf,
    worker: Option<JoinHandle<Result<()>>>,
    shutdown: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
}

impl LinuxNativeSegmentEncoder {
    pub fn start(
        config: &EncoderConfig,
        out_path: PathBuf,
        video_rx: mpsc::Receiver<Vec<u8>>,
        system_audio_rx: Option<AudioChunkReceiver>,
        microphone_rx: Option<AudioChunkReceiver>,
    ) -> Result<Self> {
        if config.audio.is_some()
            || config.mic.is_some()
            || system_audio_rx.is_some()
            || microphone_rx.is_some()
        {
            bail!("System audio and microphone recording are not supported on Linux yet.");
        }
        // Prepare synchronously: missing plugins or an unwritable destination
        // must fail before the UI announces a running recording.
        let prepared = PreparedEncoder::new(config, &out_path)?;
        let shutdown = Arc::new(AtomicBool::new(false));
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_shutdown = Arc::clone(&shutdown);
        let worker_cancelled = Arc::clone(&cancelled);
        let output = out_path.clone();
        let worker = std::thread::Builder::new()
            .name("kiri-linux-encoder".into())
            .spawn(move || {
                encode_bgra_mp4(
                    prepared,
                    &output,
                    video_rx,
                    worker_shutdown,
                    worker_cancelled,
                )
            })
            .context("Could not start the Linux encoder")?;
        Ok(Self {
            out_path,
            worker: Some(worker),
            shutdown,
            cancelled,
        })
    }

    pub fn finish(mut self) -> Result<PathBuf> {
        self.shutdown.store(true, Ordering::Release);
        self.worker
            .take()
            .expect("encoder worker")
            .join()
            .map_err(|_| anyhow!("The Linux encoder worker panicked."))??;
        Ok(self.out_path.clone())
    }

    pub fn cancel(mut self) {
        self.cancelled.store(true, Ordering::Release);
        self.shutdown.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            if matches!(worker.join(), Ok(Ok(()))) {
                let _ = std::fs::remove_file(&self.out_path);
            }
        }
    }
}

impl Drop for LinuxNativeSegmentEncoder {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            self.cancelled.store(true, Ordering::Release);
            self.shutdown.store(true, Ordering::Release);
            if matches!(worker.join(), Ok(Ok(()))) {
                let _ = std::fs::remove_file(&self.out_path);
            }
        }
    }
}

struct PreparedEncoder {
    pipeline: PipelineGuard,
    appsrc: gstreamer_app::AppSrc,
    bus: gstreamer::Bus,
    output: tempfile::NamedTempFile,
    width: u32,
    height: u32,
    fps: u32,
    frame_bytes: usize,
    queue_bytes: u64,
}

impl PreparedEncoder {
    fn new(config: &EncoderConfig, out_path: &Path) -> Result<Self> {
        let width = u32::try_from(config.width).context("Invalid encoder width")?;
        let height = u32::try_from(config.height).context("Invalid encoder height")?;
        let bitrate = u32::try_from(config.bitrate).context("Invalid encoder bitrate")?;
        if width < 2
            || height < 2
            || width % 2 != 0
            || height % 2 != 0
            || !(1..=120).contains(&config.fps)
            || bitrate < 1000
        {
            bail!("Invalid Linux H.264 encoder configuration.");
        }
        let frame_bytes = (width as usize)
            .checked_mul(height as usize)
            .and_then(|value| value.checked_mul(4))
            .ok_or_else(|| anyhow!("The encoder frame dimensions overflow."))?;
        let queue_bytes = (frame_bytes as u64)
            .checked_mul(2)
            .ok_or_else(|| anyhow!("The encoder queue dimensions overflow."))?;
        let fps = config.fps;
        // x264enc already produces complete AVC access units and codec_data.
        // h264parse would clear their explicit duration in GstBaseParse and
        // replace it with 1/fps, truncating a static recording's final frame.
        let pipeline = pipeline(
            &format!(
                "appsrc name=src is-live=true format=time do-timestamp=false \
             caps=video/x-raw,format=BGRA,width={width},height={height},framerate={fps}/1 ! \
             videoconvert ! video/x-raw,format=I420 ! \
             x264enc tune=zerolatency speed-preset=superfast bitrate={} key-int-max={} ! \
             video/x-h264,profile=baseline,stream-format=avc,alignment=au ! mp4mux ! filesink name=output",
                bitrate / 1000,
                fps * 2,
            ),
            "H.264 MP4 encoding",
        )?;
        let output = staged_output(out_path)?;
        set_file(&pipeline, "output", output.path())?;
        let appsrc = pipeline
            .by_name("src")
            .ok_or_else(|| anyhow!("The encoder appsrc is missing."))?
            .downcast::<gstreamer_app::AppSrc>()
            .map_err(|_| anyhow!("The encoder source has the wrong type."))?;
        appsrc.set_block(false);
        appsrc.set_max_bytes(queue_bytes);
        let bus = pipeline_bus(&pipeline)?;
        pipeline
            .set_state(gstreamer::State::Playing)
            .context("Could not start the Linux MP4 encoder")?;
        check_pipeline(&bus, "Linux MP4 encoding")?;
        Ok(Self {
            pipeline,
            appsrc,
            bus,
            output,
            width,
            height,
            fps,
            frame_bytes,
            queue_bytes,
        })
    }

    fn has_room(&self) -> bool {
        // Only this worker pushes buffers; the streaming thread only removes
        // them. This preflight therefore enforces a hard two-frame bound even
        // on GStreamer versions where max-bytes is merely a notification.
        queue_has_room(
            self.appsrc.current_level_bytes(),
            self.frame_bytes as u64,
            self.queue_bytes,
        )
    }

    fn wait_for_room(&self) -> Result<()> {
        let deadline = Instant::now() + FINALIZE_TIMEOUT;
        while !self.has_room() {
            check_pipeline(&self.bus, "Linux MP4 encoding")?;
            if Instant::now() >= deadline {
                bail!("The Linux MP4 encoder stopped accepting frames.");
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(())
    }

    fn push(&self, pixels: Vec<u8>, pts: Duration, duration: Duration) -> Result<()> {
        let mut buffer = gstreamer::Buffer::from_mut_slice(pixels);
        let buffer_ref = buffer
            .get_mut()
            .ok_or_else(|| anyhow!("The encoder buffer is not writable."))?;
        buffer_ref.set_pts(clock_time(pts));
        buffer_ref.set_duration(clock_time(duration));
        self.appsrc
            .push_buffer(buffer)
            .context("The Linux encoder rejected a video frame")?;
        Ok(())
    }
}

fn encode_bgra_mp4(
    prepared: PreparedEncoder,
    out_path: &Path,
    video_rx: mpsc::Receiver<Vec<u8>>,
    shutdown: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
) -> Result<()> {
    let mut pending = None::<Vec<u8>>;
    let mut origin = None::<Instant>;
    let mut timeline = FrameTimeline::new(prepared.fps);
    loop {
        if cancelled.load(Ordering::Acquire) {
            bail!("Linux recording was cancelled.");
        }
        check_pipeline(&prepared.bus, "Linux MP4 encoding")?;
        let frame = if shutdown.load(Ordering::Acquire) {
            match video_rx.try_recv() {
                Ok(frame) => frame,
                Err(_) => break,
            }
        } else {
            match video_rx.recv_timeout(INPUT_POLL) {
                Ok(frame) => frame,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        };
        if frame.len() != prepared.frame_bytes {
            bail!("The Linux capture frame size changed during recording.");
        }
        let start = *origin.get_or_insert_with(Instant::now);
        if pending.is_none() {
            pending = Some(frame);
            continue;
        }
        if !prepared.has_room() {
            continue;
        }
        if let Some((pts, duration)) = timeline.advance(start.elapsed()) {
            prepared.push(
                pending.replace(frame).expect("pending video frame"),
                pts,
                duration,
            )?;
        }
    }
    let start = origin.ok_or_else(|| anyhow!("The Linux recorder produced no video frames."))?;
    // Capture stop closes the source before finish; measure this boundary
    // before flushing so mux/encoder latency never extends the recording.
    let (pts, duration) = timeline.finish(start.elapsed());
    prepared.wait_for_room()?;
    prepared.push(pending.expect("first video frame"), pts, duration)?;
    prepared
        .appsrc
        .end_of_stream()
        .context("Could not finish the Linux encoder input")?;
    wait_for_eos(&prepared.bus, FINALIZE_TIMEOUT, "Linux MP4 encoding")?;
    let PreparedEncoder {
        pipeline,
        output,
        width,
        height,
        ..
    } = prepared;
    drop(pipeline);
    validate_recording(output.path(), Some((width, height)))?;
    if cancelled.load(Ordering::Acquire) {
        bail!("Linux recording was cancelled.");
    }
    output
        .persist_noclobber(out_path)
        .map_err(|error| anyhow!("Could not publish the finished recording: {}", error.error))?;
    Ok(())
}

pub fn probe_video(video: &Path) -> Option<(i64, i64, Option<f64>)> {
    let info = discover(video).ok()?;
    let streams = info.video_streams();
    let stream = streams.first()?;
    Some((
        i64::from(stream.width()),
        i64::from(stream.height()),
        info.duration()
            .map(|time| time.nseconds() as f64 / 1_000_000_000.0),
    ))
}

fn discover(video: &Path) -> Result<gstreamer_pbutils::DiscovererInfo> {
    gstreamer::init()?;
    let discoverer = gstreamer_pbutils::Discoverer::new(gstreamer::ClockTime::from_seconds(5))?;
    let uri = glib::filename_to_uri(video, None)?;
    let info = discoverer.discover_uri(&uri)?;
    if info.result() != gstreamer_pbutils::DiscovererResult::Ok {
        bail!("The media file could not be completely inspected.");
    }
    Ok(info)
}

fn validate_recording(video: &Path, expected: Option<(u32, u32)>) -> Result<(u32, u32, f64)> {
    use std::io::Read;
    let mut header = [0; 12];
    std::fs::File::open(video)?.read_exact(&mut header)?;
    if &header[4..8] != b"ftyp" {
        bail!("The recording is not an MP4 container.");
    }
    let info = discover(video)?;
    let streams = info.video_streams();
    if streams.len() != 1 || !info.audio_streams().is_empty() {
        bail!("The Linux recording contains unexpected media tracks.");
    }
    let stream = &streams[0];
    let dimensions = (stream.width(), stream.height());
    if dimensions.0 == 0 || dimensions.1 == 0 || expected.is_some_and(|size| size != dimensions) {
        bail!("The finalized recording has invalid dimensions.");
    }
    let duration = info
        .duration()
        .filter(|value| *value > gstreamer::ClockTime::ZERO)
        .ok_or_else(|| anyhow!("The finalized MP4 has no positive duration."))?;
    Ok((
        dimensions.0,
        dimensions.1,
        duration.nseconds() as f64 / 1_000_000_000.0,
    ))
}

pub fn merge_segments(segments: &[PathBuf], out_path: &Path) -> Result<()> {
    if segments.is_empty() {
        bail!("No recording segments to merge.");
    }
    let first = validate_recording(&segments[0], None)?;
    let mut expected_duration = first.2;
    for segment in &segments[1..] {
        expected_duration += validate_recording(segment, Some((first.0, first.1)))?.2;
    }
    let output = staged_output(out_path)?;
    if segments.len() == 1 {
        std::fs::copy(&segments[0], output.path()).context("Could not stage the recording")?;
    } else {
        // qtdemux already supplies AVC access units, codec_data and exact MP4
        // sample durations. Preserve these through concat, including the last
        // held frame; another h264parse would replace its duration with 1/fps.
        // concat adjusts segment running times before the fresh MP4 container.
        let mut description = String::from(
            "concat name=c adjust-base=true ! video/x-h264,stream-format=avc,alignment=au ! mp4mux ! filesink name=output"
        );
        for index in 0..segments.len() {
            description.push_str(&format!(
                " filesrc name=input_{index} ! qtdemux ! video/x-h264,stream-format=avc,alignment=au ! \
                 queue max-size-buffers=2 max-size-bytes=0 max-size-time=0 ! c.sink_{index}"
            ));
        }
        let pipeline = pipeline(&description, "recording segment merge")?;
        set_file(&pipeline, "output", output.path())?;
        for (index, segment) in segments.iter().enumerate() {
            set_file(&pipeline, &format!("input_{index}"), segment)?;
        }
        let bus = pipeline_bus(&pipeline)?;
        pipeline
            .set_state(gstreamer::State::Playing)
            .context("Could not start recording segment merge")?;
        wait_for_eos(&bus, Duration::from_secs(120), "Recording segment merge")?;
        drop(pipeline);
    }
    let actual = validate_recording(output.path(), Some((first.0, first.1)))?;
    if (actual.2 - expected_duration).abs() > 0.1 {
        bail!("The merged recording duration does not match its completed segments.");
    }
    output
        .persist_noclobber(out_path)
        .map_err(|error| anyhow!("Could not publish the merged recording: {}", error.error))?;
    Ok(())
}

fn decode_pipeline(
    video: &Path,
    width: u32,
    height: u32,
    fps: Option<u32>,
) -> Result<(PipelineGuard, gstreamer_app::AppSink)> {
    let rate = fps
        .map(|fps| format!("videorate ! video/x-raw,framerate={fps}/1 ! "))
        .unwrap_or_default();
    let pipeline = pipeline(
        &format!(
            "filesrc name=input ! decodebin ! videoconvert ! videoscale ! \
         video/x-raw,format=RGBA,width={width},height={height} ! {rate}\
         appsink name=sink max-buffers=1 drop=false sync=false"
        ),
        "video decoding",
    )?;
    set_file(&pipeline, "input", video)?;
    let sink = pipeline
        .by_name("sink")
        .ok_or_else(|| anyhow!("The decoder appsink is missing."))?
        .downcast::<gstreamer_app::AppSink>()
        .map_err(|_| anyhow!("The decoder sink has the wrong type."))?;
    pipeline
        .set_state(gstreamer::State::Playing)
        .context("Could not start video decoding")?;
    Ok((pipeline, sink))
}

pub fn video_first_frame_png(video: &Path, max_long_edge: u32) -> Result<Vec<u8>> {
    let (width, height, _) =
        probe_video(video).ok_or_else(|| anyhow!("Could not inspect the video."))?;
    let (width, height) =
        scale_long_edge(u32::try_from(width)?, u32::try_from(height)?, max_long_edge);
    let (pipeline, sink) = decode_pipeline(video, width, height, None)?;
    let sample = match sink.try_pull_sample(gstreamer::ClockTime::from_seconds(10)) {
        Some(sample) => sample,
        None => {
            check_pipeline(&pipeline_bus(&pipeline)?, "Thumbnail decoding")?;
            bail!("No thumbnail frame was decoded.");
        }
    };
    let pixels = packed_sample_pixels(
        &sample,
        PixelCrop {
            x: 0,
            y: 0,
            width,
            height,
        },
    )?;
    drop(pipeline);
    let rgba = image::RgbaImage::from_raw(width, height, pixels)
        .ok_or_else(|| anyhow!("The thumbnail pixel buffer size is invalid."))?;
    let mut encoded = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(rgba).write_to(&mut encoded, image::ImageFormat::Png)?;
    Ok(encoded.into_inner())
}

pub fn export_gif(
    video: &Path,
    max_long_edge: u32,
    fps: u32,
) -> Result<(PathBuf, i64, i64, Option<f64>)> {
    let mut clock = crate::core::gif_timing::GifFrameClock::new(fps)
        .ok_or_else(|| anyhow!("The GIF frame rate is invalid."))?;
    let (width, height, duration) =
        probe_video(video).ok_or_else(|| anyhow!("Could not inspect the source video."))?;
    let _duration = duration
        .filter(|duration| duration.is_finite() && *duration > 0.0)
        .ok_or_else(|| anyhow!("The source video has no positive duration."))?;
    let (width, height) =
        scale_long_edge(u32::try_from(width)?, u32::try_from(height)?, max_long_edge);
    let out_path = std::env::temp_dir().join(format!(
        "kiri-linux-gif-{}.gif",
        uuid::Uuid::new_v4().as_simple()
    ));
    let output = tempfile::Builder::new()
        .prefix(".kiri-gif-")
        .suffix(".gif")
        .tempfile()?;
    let (pipeline, sink) = decode_pipeline(video, width, height, Some(fps))?;
    let bus = pipeline_bus(&pipeline)?;
    let mut frame_count = 0usize;
    {
        let mut writer = std::io::BufWriter::new(output.reopen()?);
        let mut encoder = image::codecs::gif::GifEncoder::new_with_speed(&mut writer, 10);
        encoder.set_repeat(image::codecs::gif::Repeat::Infinite)?;
        loop {
            // A bounded appsink plus one encoded frame keeps memory independent
            // of recording length. No optional external gifenc plugin is needed.
            let Some(sample) = sink.try_pull_sample(gstreamer::ClockTime::from_seconds(15)) else {
                if let Some(message) = bus.pop_filtered(&[gstreamer::MessageType::Error]) {
                    return Err(message_failure(&message, "GIF decoding"));
                }
                if sink.is_eos() {
                    break;
                }
                bail!("GIF decoding stopped delivering video frames.");
            };
            let pixels = packed_sample_pixels(
                &sample,
                PixelCrop {
                    x: 0,
                    y: 0,
                    width,
                    height,
                },
            )?;
            let rgba = image::RgbaImage::from_raw(width, height, pixels)
                .ok_or_else(|| anyhow!("The GIF frame size is invalid."))?;
            encoder.encode_frame(image::Frame::from_parts(
                rgba,
                0,
                0,
                image::Delay::from_numer_denom_ms(clock.next_delay_ms(), 1),
            ))?;
            frame_count += 1;
        }
        drop(encoder);
        std::io::Write::flush(&mut writer)?;
    }
    drop(pipeline);
    if frame_count == 0 {
        bail!("GIF encoding produced no frames.");
    }
    output
        .persist_noclobber(&out_path)
        .map_err(|error| anyhow!("Could not publish the GIF: {}", error.error))?;
    Ok((
        out_path,
        i64::from(width),
        i64::from(height),
        Some(clock.duration_seconds()),
    ))
}

fn scale_long_edge(width: u32, height: u32, max_long_edge: u32) -> (u32, u32) {
    if width == 0 || height == 0 || max_long_edge == 0 {
        return (1, 1);
    }
    if width.max(height) <= max_long_edge {
        return (width, height);
    }
    if width >= height {
        (
            max_long_edge,
            (u64::from(height) * u64::from(max_long_edge) / u64::from(width)).max(1) as u32,
        )
    } else {
        (
            (u64::from(width) * u64::from(max_long_edge) / u64::from(height)).max(1) as u32,
            max_long_edge,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::AnimationDecoder;

    /// Copy only this test's isolated fixtures before TempDir drops, including
    /// during an assertion panic. A failed native check must leave evidence.
    struct ReviewArtifacts {
        source: PathBuf,
        name: &'static str,
    }

    impl ReviewArtifacts {
        fn new(source: &Path, name: &'static str) -> Self {
            Self {
                source: source.to_owned(),
                name,
            }
        }
    }

    impl Drop for ReviewArtifacts {
        fn drop(&mut self) {
            let Some(root) = std::env::var_os("KIRI_LINUX_MEDIA_QA_DIR") else {
                return;
            };
            let destination = PathBuf::from(root).join(self.name);
            if std::fs::create_dir_all(&destination).is_err() {
                return;
            }
            let _ = std::fs::write(
                destination.join("test-status.txt"),
                if std::thread::panicking() {
                    "failed\n"
                } else {
                    "passed\n"
                },
            );
            if let Ok(entries) = std::fs::read_dir(&self.source) {
                for entry in entries.flatten() {
                    if entry.file_type().is_ok_and(|kind| kind.is_file()) {
                        let _ = std::fs::copy(entry.path(), destination.join(entry.file_name()));
                    }
                }
            }
        }
    }

    fn display() -> DisplayIdentity {
        DisplayIdentity {
            device_name: "test-display".into(),
            physical_x: 0,
            physical_y: 0,
            physical_width: 2560,
            physical_height: 1440,
            scale_factor: 2.0,
        }
    }

    #[test]
    fn portal_geometry_uses_compositor_coordinates_and_rejects_another_display() {
        assert!(validate_portal_display(&display(), Some((0, 0)), Some((1280, 720))).is_ok());
        assert!(validate_portal_display(&display(), Some((1280, 0)), Some((1280, 720))).is_err());
        assert!(validate_portal_display(&display(), None, Some((1920, 1080))).is_err());
        assert!(validate_portal_display(&display(), None, None).is_ok());
    }

    #[test]
    fn cropped_pixels_follow_stride_and_reject_truncated_buffers() {
        let pixels = [
            1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 0, 9, 10, 11, 12, 13, 14, 15, 16, 0, 0, 0, 0,
        ];
        let crop = PixelCrop {
            x: 1,
            y: 0,
            width: 1,
            height: 2,
        };
        assert_eq!(
            crop_bgra_frame(&pixels, 12, 2, 2, crop),
            Some(vec![5, 6, 7, 8, 13, 14, 15, 16])
        );
        assert!(crop_bgra_frame(&pixels[..19], 12, 2, 2, crop).is_none());
        assert!(crop_bgra_frame(&pixels, 7, 2, 2, crop).is_none());
        assert!(crop_bgra_frame(
            &pixels,
            12,
            2,
            2,
            PixelCrop {
                x: u32::MAX,
                ..crop
            }
        )
        .is_none());
    }

    #[test]
    fn recording_crop_rejects_stale_display_size_and_invalid_coordinates() {
        assert!(capture_crop(Rect::new(20.0, 10.0, 100.0, 80.0), 2.0, 2560, 1440).is_ok());
        assert!(capture_crop(Rect::new(1200.0, 10.0, 100.0, 80.0), 2.0, 2560, 1440).is_err());
        assert!(capture_crop(Rect::new(-1.0, 10.0, 100.0, 80.0), 2.0, 2560, 1440).is_err());
        assert!(capture_crop(Rect::new(f64::NAN, 10.0, 100.0, 80.0), 2.0, 2560, 1440).is_err());
    }

    #[tokio::test]
    async fn portal_wait_is_cancelled_and_deadlined_without_a_running_desktop() {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_for_task = Arc::clone(&stop);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            stop_for_task.store(true, Ordering::Release);
        });
        let start = Instant::now();
        let result = portal_step(
            &stop,
            tokio::time::Instant::now() + Duration::from_secs(120),
            "test",
            std::future::pending::<Result<(), anyhow::Error>>(),
        )
        .await;
        assert!(result.unwrap_err().to_string().contains("cancelled"));
        assert!(start.elapsed() < Duration::from_secs(2));
        let result = portal_step(
            &AtomicBool::new(false),
            tokio::time::Instant::now(),
            "test",
            std::future::pending::<Result<(), anyhow::Error>>(),
        )
        .await;
        assert!(result.unwrap_err().to_string().contains("timed out"));
    }

    fn config() -> EncoderConfig {
        EncoderConfig {
            width: 64,
            height: 48,
            fps: 30,
            bitrate: 1_000_000,
            audio: None,
            mic: None,
        }
    }

    fn solid_frame(bgra: [u8; 4]) -> Vec<u8> {
        bgra.repeat(64 * 48)
    }

    fn fixture(path: &Path, bgra: [u8; 4], duration_ms: u64) {
        let encoder = PreparedEncoder::new(&config(), path).unwrap();
        encoder
            .push(
                solid_frame(bgra),
                Duration::ZERO,
                Duration::from_millis(duration_ms),
            )
            .unwrap();
        encoder.appsrc.end_of_stream().unwrap();
        let finalized = wait_for_eos(&encoder.bus, FINALIZE_TIMEOUT, "Fixture encoding");
        let PreparedEncoder {
            pipeline, output, ..
        } = encoder;
        drop(pipeline);
        output.persist_noclobber(path).unwrap();
        finalized.unwrap();
        validate_recording(path, Some((64, 48))).unwrap();
    }

    fn decoded_colors(video: &Path) -> Vec<[u8; 3]> {
        let (pipeline, sink) = decode_pipeline(video, 64, 48, None).unwrap();
        let bus = pipeline_bus(&pipeline).unwrap();
        let mut pixels = Vec::new();
        loop {
            if let Some(sample) = sink.try_pull_sample(gstreamer::ClockTime::from_seconds(5)) {
                let rgba = packed_sample_pixels(
                    &sample,
                    PixelCrop {
                        x: 0,
                        y: 0,
                        width: 64,
                        height: 48,
                    },
                )
                .unwrap();
                pixels.push([rgba[0], rgba[1], rgba[2]]);
            } else {
                assert!(sink.is_eos(), "Decoder did not complete");
                if let Some(message) = bus.pop_filtered(&[gstreamer::MessageType::Error]) {
                    panic!("{}", message_failure(&message, "Test decoding"));
                }
                break;
            }
        }
        pixels
    }

    /// Runs on Linux CI against real installed GStreamer plugins. Fixtures are
    /// isolated from the capture library and require no desktop or camera.
    #[test]
    fn native_mp4_segments_keep_timing_and_merge() {
        let temp = tempfile::tempdir().unwrap();
        let _review = ReviewArtifacts::new(temp.path(), "segments");
        let first = temp.path().join("first.mp4");
        let second = temp.path().join("second.mp4");
        let merged = temp.path().join("merged.mp4");
        fixture(&first, [0, 0, 255, 255], 400);
        fixture(&second, [255, 0, 0, 255], 600);
        let first_duration = validate_recording(&first, Some((64, 48))).unwrap().2;
        let second_duration = validate_recording(&second, Some((64, 48))).unwrap().2;
        std::fs::write(
            temp.path().join("segment-durations.txt"),
            format!("first={first_duration}\nsecond={second_duration}\n"),
        )
        .unwrap();
        assert!(
            (first_duration - 0.4).abs() < 0.03,
            "First segment duration was {first_duration}"
        );
        assert!(
            (second_duration - 0.6).abs() < 0.03,
            "Second segment duration was {second_duration}"
        );
        merge_segments(&[first.clone(), second.clone()], &merged).unwrap();
        let duration = validate_recording(&merged, Some((64, 48))).unwrap().2;
        std::fs::write(
            temp.path().join("merged-duration.txt"),
            format!("{duration}\n"),
        )
        .unwrap();
        assert!(
            (duration - 1.0).abs() < 0.05,
            "Merged duration was {duration}"
        );
        let colors = decoded_colors(&merged);
        assert_eq!(colors.len(), 2);
        assert!(
            colors[0][0] > 220 && colors[0][2] < 30,
            "First segment was not red: {:?}",
            colors[0]
        );
        assert!(
            colors[1][2] > 220 && colors[1][0] < 30,
            "Second segment was not blue: {:?}",
            colors[1]
        );
        let thumbnail = video_first_frame_png(&merged, 32).unwrap();
        std::fs::write(temp.path().join("first-frame.png"), &thumbnail).unwrap();
        let image = image::load_from_memory(&thumbnail).unwrap();
        assert_eq!((image.width(), image.height()), (32, 24));
        let pixel = image.to_rgb8().get_pixel(0, 0).0;
        assert!(pixel[0] > 220 && pixel[2] < 30);
        let (gif, width, height, gif_duration) = export_gif(&merged, 32, 12).unwrap();
        let staged_gif = temp.path().join("merged.gif");
        std::fs::rename(gif, &staged_gif).unwrap();
        assert_eq!((width, height), (32, 24));
        let decoder = image::codecs::gif::GifDecoder::new(std::io::BufReader::new(
            std::fs::File::open(&staged_gif).unwrap(),
        ))
        .unwrap();
        let frames = decoder.into_frames().collect_frames().unwrap();
        assert!(
            frames.len() >= 10,
            "Expected one second of GIF frames, got {}",
            frames.len()
        );
        let gif_seconds: f64 = frames
            .iter()
            .map(|frame| {
                let (numerator, denominator) = frame.delay().numer_denom_ms();
                f64::from(numerator) / f64::from(denominator) / 1000.0
            })
            .sum();
        assert!((gif_seconds - frames.len() as f64 / 12.0).abs() <= 0.005_000_001);
        assert!((gif_duration.unwrap() - gif_seconds).abs() < 0.000_001);
        assert!((gif_seconds - duration).abs() < 1.0 / 12.0 + 0.01);
        std::fs::write(temp.path().join("metadata.txt"), format!("width=64\nheight=48\nduration={duration}\nsegment_colors={colors:?}\ngif_frames={}\n", frames.len())).unwrap();
    }

    #[test]
    fn native_encoder_preserves_static_screen_time_and_rejects_partial_output() {
        let temp = tempfile::tempdir().unwrap();
        let _review = ReviewArtifacts::new(temp.path(), "static-screen");
        let output = temp.path().join("static.mp4");
        let (tx, rx) = mpsc::sync_channel(2);
        let encoder =
            LinuxNativeSegmentEncoder::start(&config(), output.clone(), rx, None, None).unwrap();
        tx.send(solid_frame([0, 255, 0, 255])).unwrap();
        std::thread::sleep(Duration::from_millis(250));
        tx.send(solid_frame([0, 255, 0, 255])).unwrap();
        std::thread::sleep(Duration::from_millis(250));
        drop(tx);
        encoder.finish().unwrap();
        let duration = validate_recording(&output, Some((64, 48))).unwrap().2;
        assert!(
            duration > 0.4 && duration < 2.0,
            "A static 500ms screen was encoded as {duration}s"
        );

        let failed = temp.path().join("invalid.mp4");
        let (tx, rx) = mpsc::sync_channel(2);
        let encoder =
            LinuxNativeSegmentEncoder::start(&config(), failed.clone(), rx, None, None).unwrap();
        tx.send(vec![0; 3]).unwrap();
        drop(tx);
        assert!(encoder.finish().is_err());
        assert!(!failed.exists());
        assert_eq!(
            std::fs::read_dir(temp.path()).unwrap().count(),
            1,
            "A failed encoder left a staging file"
        );
    }

    #[test]
    fn merge_failure_keeps_original_segments_and_existing_destination() {
        let temp = tempfile::tempdir().unwrap();
        let _review = ReviewArtifacts::new(temp.path(), "merge-failure");
        let first = temp.path().join("first.mp4");
        fixture(&first, [0, 0, 255, 255], 400);
        let bytes = std::fs::read(&first).unwrap();
        let output = temp.path().join("output.mp4");
        assert!(
            merge_segments(&[first.clone(), temp.path().join("missing.mp4")], &output).is_err()
        );
        assert!(!output.exists());
        assert_eq!(std::fs::read(&first).unwrap(), bytes);
        std::fs::write(&output, b"existing destination").unwrap();
        assert!(merge_segments(std::slice::from_ref(&first), &output).is_err());
        assert_eq!(std::fs::read(&output).unwrap(), b"existing destination");
        assert_eq!(std::fs::read(&first).unwrap(), bytes);
    }

    #[test]
    fn media_error_and_timeout_do_not_count_as_successful_eos() {
        let error_pipeline = pipeline(
            "videotestsrc num-buffers=2 ! identity error-after=1 ! fakesink",
            "Error test",
        )
        .unwrap();
        let bus = pipeline_bus(&error_pipeline).unwrap();
        let _ = error_pipeline.set_state(gstreamer::State::Playing);
        assert!(wait_for_eos(&bus, Duration::from_secs(3), "Error test").is_err());
        let idle = pipeline_bus(&pipeline("appsrc ! fakesink", "Timeout test").unwrap()).unwrap();
        assert!(
            wait_for_eos(&idle, Duration::from_millis(1), "Timeout test")
                .unwrap_err()
                .to_string()
                .contains("timed out")
        );
    }
}
