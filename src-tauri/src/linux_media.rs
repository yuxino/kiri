//! Linux media helpers: PipeWire ScreenCast capture and GStreamer MP4 encoding.
//!
//! Uses system GStreamer plugins already installed on the host. Never downloads
//! or launches an external FFmpeg binary.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use ashpd::desktop::screencast::{CursorMode, Screencast, SourceType};
use ashpd::desktop::PersistMode;
use glib::prelude::*;
use gstreamer::prelude::*;

use crate::core::geometry::Rect;
use crate::record::{AudioChunkReceiver, EncoderConfig};

// ---------------------------------------------------------------------------
// ScreenCast → PipeWire frames
// ---------------------------------------------------------------------------

/// Capture a cropped region from a portal ScreenCast session until `stop_flag`.
pub async fn run_pipewire_region_capture(
    region: Rect,
    backing_scale: f64,
    shows_cursor: bool,
    video_tx: crate::capture::VideoFrameSender,
    stop_flag: Arc<AtomicBool>,
) -> Result<()> {
    let proxy = Screencast::new().await.map_err(|error| {
        anyhow!("Could not connect to the ScreenCast portal: {error}")
    })?;
    let session = proxy
        .create_session()
        .await
        .map_err(|error| anyhow!("Could not create a ScreenCast session: {error}"))?;

    let cursor_mode = if shows_cursor {
        CursorMode::Embedded
    } else {
        CursorMode::Hidden
    };
    proxy
        .select_sources(
            &session,
            cursor_mode,
            SourceType::Monitor.into(),
            false,
            None,
            PersistMode::DoNot,
        )
        .await
        .map_err(|error| anyhow!("Could not select ScreenCast sources: {error}"))?;

    let response = proxy
        .start(&session, None)
        .await
        .map_err(|error| anyhow!("ScreenCast start failed: {error}"))?
        .response()
        .map_err(|error| anyhow!("ScreenCast was cancelled or denied: {error}"))?;

    let stream = response
        .streams()
        .first()
        .ok_or_else(|| anyhow!("The ScreenCast portal returned no streams."))?;
    let node_id = stream.pipe_wire_node_id();
    let fd = proxy
        .open_pipe_wire_remote(&session)
        .await
        .map_err(|error| anyhow!("Could not open the PipeWire remote: {error}"))?;

    // Hand off to a blocking PipeWire pump so the async portal runtime can exit.
    std::thread::Builder::new()
        .name("kiri-pipewire-pump".into())
        .spawn({
            let stop_flag = Arc::clone(&stop_flag);
            move || pump_pipewire_frames(fd, node_id, region, backing_scale, video_tx, stop_flag)
        })
        .map_err(|error| anyhow!("Could not start the PipeWire pump: {error}"))?
        .join()
        .map_err(|_| anyhow!("The PipeWire pump panicked."))?
}

fn pump_pipewire_frames(
    fd: std::os::fd::OwnedFd,
    node_id: u32,
    region: Rect,
    backing_scale: f64,
    video_tx: crate::capture::VideoFrameSender,
    stop_flag: Arc<AtomicBool>,
) -> Result<()> {
    use std::os::fd::AsRawFd;

    let width = crate::core::policy::RecordingPolicy::pixel_dimension(region.width, backing_scale)
        .max(2) as u32;
    let height =
        crate::core::policy::RecordingPolicy::pixel_dimension(region.height, backing_scale).max(2)
            as u32;
    let crop_x = (region.x * backing_scale).round().max(0.0) as u32;
    let crop_y = (region.y * backing_scale).round().max(0.0) as u32;
    let raw_fd = fd.as_raw_fd();

    gstreamer::init().map_err(|error| anyhow!("GStreamer init failed: {error}"))?;

    let pipeline = gstreamer::parse::launch(&format!(
        "pipewiresrc fd={raw_fd} path={node_id} do-timestamp=true ! \
         videoconvert ! video/x-raw,format=BGRA ! \
         appsink name=sink max-buffers=2 drop=true sync=false"
    ))
    .map_err(|error| anyhow!("Could not build the PipeWire capture pipeline: {error}"))?;

    let pipeline = pipeline
        .downcast::<gstreamer::Pipeline>()
        .map_err(|_| anyhow!("GStreamer did not return a pipeline element."))?;
    let sink = pipeline
        .by_name("sink")
        .ok_or_else(|| anyhow!("The PipeWire appsink is missing."))?
        .downcast::<gstreamer_app::AppSink>()
        .map_err(|_| anyhow!("The PipeWire appsink has the wrong type."))?;

    pipeline
        .set_state(gstreamer::State::Playing)
        .map_err(|error| anyhow!("Could not start PipeWire capture: {error:?}"))?;

    while !stop_flag.load(Ordering::Acquire) {
        let sample = match sink.try_pull_sample(gstreamer::ClockTime::from_mseconds(50)) {
            Some(sample) => sample,
            None => continue,
        };
        let Some(caps) = sample.caps() else {
            continue;
        };
        let Some(structure) = caps.structure(0) else {
            continue;
        };
        let Ok(full_width) = structure.get::<i32>("width") else {
            continue;
        };
        let Ok(full_height) = structure.get::<i32>("height") else {
            continue;
        };
        if full_width <= 0 || full_height <= 0 {
            continue;
        }
        let Some(buffer) = sample.buffer() else {
            continue;
        };
        let Ok(map) = buffer.map_readable() else {
            continue;
        };
        // PipeWire/GStreamer frames are often padded; derive stride from the
        // buffer nbytes instead of assuming tightly packed width*4 rows.
        let Some(stride) = (map.size()).checked_div(full_height as usize) else {
            continue;
        };
        if stride < (full_width as usize).saturating_mul(4) {
            continue;
        }
        let Some(frame) = crop_bgra_frame(
            map.as_slice(),
            stride,
            full_width as u32,
            full_height as u32,
            crop_x,
            crop_y,
            width,
            height,
        ) else {
            continue;
        };
        if video_tx.try_send(frame).is_err() {
            // Encoder is behind — drop per the shared 2-frame contract.
        }
    }

    let _ = pipeline.set_state(gstreamer::State::Null);
    drop(fd);
    Ok(())
}

fn crop_bgra_frame(
    source: &[u8],
    stride: usize,
    full_width: u32,
    full_height: u32,
    crop_x: u32,
    crop_y: u32,
    width: u32,
    height: u32,
) -> Option<Vec<u8>> {
    if crop_x.saturating_add(width) > full_width || crop_y.saturating_add(height) > full_height {
        return None;
    }
    if stride < (full_width as usize).checked_mul(4)? {
        return None;
    }
    let row_bytes = (width as usize).checked_mul(4)?;
    let mut out = vec![0u8; row_bytes.checked_mul(height as usize)?];
    for row in 0..height as usize {
        let src_offset = (crop_y as usize + row)
            .checked_mul(stride)?
            .checked_add((crop_x as usize).checked_mul(4)?)?;
        let dst_offset = row.checked_mul(row_bytes)?;
        out[dst_offset..dst_offset + row_bytes]
            .copy_from_slice(source.get(src_offset..src_offset + row_bytes)?);
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// GStreamer H.264 + AAC → MP4 segment encoder
// ---------------------------------------------------------------------------

pub struct LinuxNativeSegmentEncoder {
    out_path: PathBuf,
    worker: JoinHandle<Result<()>>,
    shutdown: Arc<AtomicBool>,
}

impl LinuxNativeSegmentEncoder {
    pub fn start(
        config: &EncoderConfig,
        out_path: PathBuf,
        video_rx: mpsc::Receiver<Vec<u8>>,
        system_audio_rx: Option<AudioChunkReceiver>,
        microphone_rx: Option<AudioChunkReceiver>,
    ) -> Result<Self> {
        let _ = (system_audio_rx, microphone_rx);
        if config.audio.is_some() || config.mic.is_some() {
            log::warn!(
                "Linux recording: system audio and microphone tracks are not encoded in this release; saving silent video."
            );
        }
        gstreamer::init().map_err(|error| anyhow!("GStreamer init failed: {error}"))?;

        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_for_worker = Arc::clone(&shutdown);
        let width = u32::try_from(config.width.max(2)).unwrap_or(2);
        let height = u32::try_from(config.height.max(2)).unwrap_or(2);
        let fps = u32::try_from(config.fps.max(1)).unwrap_or(30);
        let bitrate = u32::try_from(config.bitrate.max(1_000_000)).unwrap_or(8_000_000);
        let out_path_for_worker = out_path.clone();

        let worker = std::thread::Builder::new()
            .name("kiri-linux-encoder".into())
            .spawn(move || {
                encode_bgra_mp4(
                    out_path_for_worker,
                    width,
                    height,
                    fps,
                    bitrate,
                    video_rx,
                    shutdown_for_worker,
                )
            })
            .map_err(|error| anyhow!("Could not start the Linux encoder: {error}"))?;

        Ok(Self {
            out_path,
            worker,
            shutdown,
        })
    }

    pub fn finish(self) -> Result<PathBuf> {
        self.shutdown.store(true, Ordering::Release);
        match self.worker.join() {
            Ok(Ok(())) => Ok(self.out_path),
            Ok(Err(error)) => Err(error),
            Err(_) => bail!("The Linux encoder worker panicked."),
        }
    }

    pub fn cancel(self) {
        self.shutdown.store(true, Ordering::Release);
        let _ = self.worker.join();
        let _ = std::fs::remove_file(&self.out_path);
    }
}

fn encode_bgra_mp4(
    out_path: PathBuf,
    width: u32,
    height: u32,
    fps: u32,
    bitrate: u32,
    video_rx: mpsc::Receiver<Vec<u8>>,
    shutdown: Arc<AtomicBool>,
) -> Result<()> {
    let pipeline = gstreamer::parse::launch(&format!(
        "appsrc name=src is-live=true format=time do-timestamp=true \
         caps=video/x-raw,format=BGRA,width={width},height={height},framerate={fps}/1 ! \
         videoconvert ! \
         x264enc tune=zerolatency speed-preset=superfast bitrate={} key-int-max={} ! \
         video/x-h264,profile=baseline ! \
         h264parse ! mp4mux ! \
         filesink location=\"{}\"",
        bitrate / 1000,
        fps * 2,
        escape_pipeline_path(&out_path)?
    ))
    .map_err(|error| anyhow!("Could not build the MP4 encode pipeline: {error}"))?;

    let pipeline = pipeline
        .downcast::<gstreamer::Pipeline>()
        .map_err(|_| anyhow!("GStreamer did not return a pipeline element."))?;
    let appsrc = pipeline
        .by_name("src")
        .ok_or_else(|| anyhow!("The encoder appsrc is missing."))?
        .downcast::<gstreamer_app::AppSrc>()
        .map_err(|_| anyhow!("The encoder appsrc has the wrong type."))?;

    appsrc.set_format(gstreamer::Format::Time);
    appsrc.set_block(false);
    appsrc.set_max_bytes(u64::from(width) * u64::from(height) * 4 * 2);

    pipeline
        .set_state(gstreamer::State::Playing)
        .map_err(|error| anyhow!("Could not start the MP4 encoder: {error:?}"))?;

    let frame_duration = gstreamer::ClockTime::from_nseconds(1_000_000_000 / u64::from(fps));
    let mut frame_index = 0u64;
    let expected_bytes = (width as usize).saturating_mul(height as usize).saturating_mul(4);

    loop {
        if shutdown.load(Ordering::Acquire) {
            // Drain any final frames that arrived before stop without blocking forever.
            while let Ok(frame) = video_rx.try_recv() {
                push_frame(&appsrc, &frame, expected_bytes, frame_index, frame_duration)?;
                frame_index += 1;
            }
            break;
        }

        match video_rx.recv_timeout(Duration::from_millis(25)) {
            Ok(frame) => {
                push_frame(&appsrc, &frame, expected_bytes, frame_index, frame_duration)?;
                frame_index += 1;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    let _ = appsrc.end_of_stream();
    let bus = pipeline
        .bus()
        .ok_or_else(|| anyhow!("The encoder pipeline bus is missing."))?;
    let _ = bus.timed_pop_filtered(
        gstreamer::ClockTime::from_seconds(15),
        &[gstreamer::MessageType::Eos, gstreamer::MessageType::Error],
    );
    let _ = pipeline.set_state(gstreamer::State::Null);

    if frame_index == 0 {
        bail!("The Linux recorder produced no video frames.");
    }
    if !out_path.is_file() {
        bail!("The Linux encoder did not write an MP4 file.");
    }
    Ok(())
}

fn push_frame(
    appsrc: &gstreamer_app::AppSrc,
    frame: &[u8],
    expected_bytes: usize,
    frame_index: u64,
    frame_duration: gstreamer::ClockTime,
) -> Result<()> {
    if frame.len() < expected_bytes {
        return Ok(());
    }
    let mut buffer = gstreamer::Buffer::with_size(expected_bytes)
        .map_err(|_| anyhow!("Could not allocate an encoder buffer."))?;
    {
        let buffer_ref = buffer
            .get_mut()
            .ok_or_else(|| anyhow!("Encoder buffer is not writable."))?;
        buffer_ref.set_pts(frame_duration.saturating_mul(frame_index));
        buffer_ref.set_duration(frame_duration);
        let mut map = buffer_ref
            .map_writable()
            .map_err(|_| anyhow!("Could not map the encoder buffer."))?;
        map.copy_from_slice(&frame[..expected_bytes]);
    }
    match appsrc.push_buffer(buffer) {
        Ok(_) | Err(gstreamer::FlowError::Flushing) => Ok(()),
        Err(error) => Err(anyhow!("Encoder appsrc rejected a frame: {error}")),
    }
}

fn escape_pipeline_path(path: &Path) -> Result<String> {
    let raw = path
        .to_str()
        .ok_or_else(|| anyhow!("The output path is not valid UTF-8."))?;
    Ok(raw.replace('\\', "\\\\").replace('"', "\\\""))
}

pub fn probe_video(video: &Path) -> Option<(i64, i64, Option<f64>)> {
    gstreamer::init().ok()?;
    let discoverer = gstreamer_pbutils::Discoverer::new(gstreamer::ClockTime::from_seconds(5)).ok()?;
    let uri = glib::filename_to_uri(video, None).ok()?;
    let info = discoverer.discover_uri(&uri).ok()?;
    let video_streams = info.video_streams();
    let stream = video_streams.first()?;
    let width = i64::from(stream.width());
    let height = i64::from(stream.height());
    let duration = info.duration().map(|time| time.nseconds() as f64 / 1_000_000_000.0);
    Some((width, height, duration))
}

pub fn merge_segments(segments: &[PathBuf], out_path: &Path) -> Result<()> {
    if segments.is_empty() {
        bail!("No recording segments to merge.");
    }
    if segments.len() == 1 {
        std::fs::copy(&segments[0], out_path)
            .map_err(|error| anyhow!("Could not stage the recording: {error}"))?;
        return Ok(());
    }
    // Pause/resume multi-segment merge is deferred; concatenate via GStreamer.
    gstreamer::init().map_err(|error| anyhow!("GStreamer init failed: {error}"))?;
    let mut concat = String::from("concat name=c ! queue ! filesink location=\"");
    concat.push_str(&escape_pipeline_path(out_path)?);
    concat.push('"');
    for segment in segments {
        concat.push_str(&format!(
            " filesrc location=\"{}\" ! qtdemux ! h264parse ! c.",
            escape_pipeline_path(segment)?
        ));
    }
    let pipeline = gstreamer::parse::launch(&concat)
        .map_err(|error| anyhow!("Could not build the merge pipeline: {error}"))?;
    let pipeline = pipeline
        .downcast::<gstreamer::Pipeline>()
        .map_err(|_| anyhow!("GStreamer did not return a merge pipeline."))?;
    pipeline
        .set_state(gstreamer::State::Playing)
        .map_err(|error| anyhow!("Could not start the merge pipeline: {error:?}"))?;
    let bus = pipeline
        .bus()
        .ok_or_else(|| anyhow!("The merge pipeline bus is missing."))?;
    let _ = bus.timed_pop_filtered(
        gstreamer::ClockTime::from_seconds(120),
        &[gstreamer::MessageType::Eos, gstreamer::MessageType::Error],
    );
    let _ = pipeline.set_state(gstreamer::State::Null);
    if !out_path.is_file() {
        bail!("The merge pipeline did not produce an MP4.");
    }
    Ok(())
}

pub fn video_first_frame_png(video: &Path, max_long_edge: u32) -> Result<Vec<u8>> {
    gstreamer::init().map_err(|error| anyhow!("GStreamer init failed: {error}"))?;
    let pipeline = gstreamer::parse::launch(&format!(
        "filesrc location=\"{}\" ! decodebin ! videoconvert ! \
         videoscale ! video/x-raw,format=RGBA ! \
         appsink name=sink max-buffers=1 drop=true sync=false",
        escape_pipeline_path(video)?
    ))
    .map_err(|error| anyhow!("Could not build the thumbnail pipeline: {error}"))?;
    let pipeline = pipeline
        .downcast::<gstreamer::Pipeline>()
        .map_err(|_| anyhow!("GStreamer did not return a thumbnail pipeline."))?;
    let sink = pipeline
        .by_name("sink")
        .ok_or_else(|| anyhow!("Thumbnail appsink missing."))?
        .downcast::<gstreamer_app::AppSink>()
        .map_err(|_| anyhow!("Thumbnail appsink has the wrong type."))?;
    pipeline
        .set_state(gstreamer::State::Playing)
        .map_err(|error| anyhow!("Could not start thumbnail decode: {error:?}"))?;
    let sample = sink
        .try_pull_sample(gstreamer::ClockTime::from_seconds(10))
        .ok_or_else(|| anyhow!("No thumbnail frame was decoded."))?;
    let caps = sample
        .caps()
        .ok_or_else(|| anyhow!("Thumbnail sample has no caps."))?;
    let structure = caps
        .structure(0)
        .ok_or_else(|| anyhow!("Thumbnail caps are empty."))?;
    let width = structure
        .get::<i32>("width")
        .map_err(|_| anyhow!("Thumbnail width missing."))? as u32;
    let height = structure
        .get::<i32>("height")
        .map_err(|_| anyhow!("Thumbnail height missing."))? as u32;
    let buffer = sample
        .buffer()
        .ok_or_else(|| anyhow!("Thumbnail sample has no buffer."))?;
    let map = buffer
        .map_readable()
        .map_err(|_| anyhow!("Could not read the thumbnail buffer."))?;
    let rgba = image::RgbaImage::from_raw(width, height, map.as_slice().to_vec())
        .ok_or_else(|| anyhow!("Thumbnail pixel buffer size mismatch."))?;
    let _ = pipeline.set_state(gstreamer::State::Null);
    let dynamic = image::DynamicImage::ImageRgba8(rgba);
    let (scaled_w, scaled_h) = scale_long_edge(width, height, max_long_edge);
    let resized = dynamic.thumbnail(scaled_w, scaled_h);
    let mut encoded = std::io::Cursor::new(Vec::new());
    resized
        .write_to(&mut encoded, image::ImageFormat::Png)
        .map_err(|error| anyhow!("Could not encode the thumbnail PNG: {error}"))?;
    Ok(encoded.into_inner())
}

pub fn export_gif(video: &Path, max_long_edge: u32, fps: u32) -> Result<(PathBuf, i64, i64, Option<f64>)> {
    let (width, height, duration) = probe_video(video).unwrap_or((0, 0, None));
    let (scaled_w, scaled_h) = scale_long_edge(
        u32::try_from(width.max(1)).unwrap_or(1),
        u32::try_from(height.max(1)).unwrap_or(1),
        max_long_edge,
    );
    let out_path = std::env::temp_dir().join(format!(
        "kiri-linux-gif-{}.gif",
        uuid::Uuid::new_v4().as_simple()
    ));
    gstreamer::init().map_err(|error| anyhow!("GStreamer init failed: {error}"))?;
    let pipeline = gstreamer::parse::launch(&format!(
        "filesrc location=\"{}\" ! decodebin ! videoconvert ! \
         videoscale ! video/x-raw,width={scaled_w},height={scaled_h} ! \
         videorate ! video/x-raw,framerate={fps}/1 ! \
         videoconvert ! gifenc ! filesink location=\"{}\"",
        escape_pipeline_path(video)?,
        escape_pipeline_path(&out_path)?
    ))
    .map_err(|error| anyhow!("Could not build the GIF pipeline: {error}"))?;
    let pipeline = pipeline
        .downcast::<gstreamer::Pipeline>()
        .map_err(|_| anyhow!("GStreamer did not return a GIF pipeline."))?;
    pipeline
        .set_state(gstreamer::State::Playing)
        .map_err(|error| anyhow!("Could not start GIF encoding: {error:?}"))?;
    let bus = pipeline
        .bus()
        .ok_or_else(|| anyhow!("The GIF pipeline bus is missing."))?;
    let _ = bus.timed_pop_filtered(
        gstreamer::ClockTime::from_seconds(300),
        &[gstreamer::MessageType::Eos, gstreamer::MessageType::Error],
    );
    let _ = pipeline.set_state(gstreamer::State::Null);
    if !out_path.is_file() {
        bail!("GIF encoding did not produce a file.");
    }
    Ok((
        out_path,
        i64::from(scaled_w),
        i64::from(scaled_h),
        duration,
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
        let scaled_height =
            (u64::from(height) * u64::from(max_long_edge) / u64::from(width)).max(1) as u32;
        (max_long_edge, scaled_height)
    } else {
        let scaled_width =
            (u64::from(width) * u64::from(max_long_edge) / u64::from(height)).max(1) as u32;
        (scaled_width, max_long_edge)
    }
}
