//! Recording encoders backed by AVFoundation on macOS and Media Foundation on
//! Windows, producing the application's H.264 + AAC MP4 output.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

use crate::core::policy::RecordingPolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioSampleFormat {
    F32,
    #[cfg(windows)]
    I16,
    #[cfg(windows)]
    U16,
}

impl AudioSampleFormat {
    fn bytes_per_sample(self) -> u64 {
        match self {
            Self::F32 => 4,
            #[cfg(windows)]
            Self::I16 | Self::U16 => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioSpec {
    pub sample_rate: u32,
    pub channels: u32,
    pub format: AudioSampleFormat,
}

impl AudioSpec {
    fn queue_byte_budget(self, latency: Duration) -> Result<usize> {
        if self.sample_rate == 0 || self.channels == 0 {
            bail!("Audio capture reported an invalid stream format.");
        }
        let bytes_per_second = u128::from(self.sample_rate)
            .checked_mul(u128::from(self.channels))
            .and_then(|value| value.checked_mul(u128::from(self.format.bytes_per_sample())))
            .context("audio stream format is too large")?;
        let byte_nanos = bytes_per_second
            .checked_mul(latency.as_nanos())
            .context("audio queue budget is too large")?;
        let bytes = byte_nanos.div_ceil(1_000_000_000).max(1);
        usize::try_from(bytes).context("audio queue budget does not fit this platform")
    }
}

/// Native audio callbacks must never wait for an encoder: doing so can stall the
/// OS capture queue (and, on macOS, video delivery on the same serial queue).
/// Keep at most a short slice of PCM per input. If this queue ever overflows,
/// the segment is rejected at finalization instead of silently saving audio
/// whose content timeline may no longer match the video.
pub const AUDIO_QUEUE_MAX_LATENCY: Duration = Duration::from_millis(250);
/// Queue + pipe hand-off older than this is no longer safe to timestamp as
/// current audio. It is deliberately below the storage bound so a segment is
/// rejected before a near-full 250 ms backlog can be saved as audible drift.
pub const AUDIO_QUEUE_MAX_RESIDENCE: Duration = Duration::from_millis(150);
const AUDIO_QUEUE_MAX_CHUNKS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioQueueSendError {
    Closed,
    Unconfigured,
    Overloaded,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct AudioQueueSnapshot {
    queued_bytes: usize,
    dropped_chunks: u64,
    dropped_bytes: u64,
    late_chunks: u64,
    max_residence: Duration,
    capture_failed: bool,
    writer_failed: bool,
}

struct QueuedAudioChunk {
    bytes: Vec<u8>,
    enqueued_at: Option<Instant>,
}

struct AudioQueueState {
    chunks: VecDeque<QueuedAudioChunk>,
    queued_bytes: usize,
    max_queued_bytes: Option<usize>,
    max_chunks: usize,
    sender_count: usize,
    receiver_open: bool,
    consumer_attached: bool,
    dropped_chunks: u64,
    dropped_bytes: u64,
    late_chunks: u64,
    max_residence: Duration,
    capture_failed: bool,
    writer_failed: bool,
}

struct AudioQueueShared {
    state: Mutex<AudioQueueState>,
    ready: Condvar,
}

#[derive(Clone)]
struct AudioQueueStatus {
    shared: Arc<AudioQueueShared>,
}

impl AudioQueueStatus {
    fn snapshot(&self) -> AudioQueueSnapshot {
        let state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        AudioQueueSnapshot {
            queued_bytes: state.queued_bytes,
            dropped_chunks: state.dropped_chunks,
            dropped_bytes: state.dropped_bytes,
            late_chunks: state.late_chunks,
            max_residence: state.max_residence,
            capture_failed: state.capture_failed,
            writer_failed: state.writer_failed,
        }
    }
}

pub struct AudioChunkSender {
    shared: Arc<AudioQueueShared>,
}

impl Clone for AudioChunkSender {
    fn clone(&self) -> Self {
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.sender_count += 1;
        drop(state);
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl AudioChunkSender {
    /// Configures the byte bound from the native PCM format before callbacks
    /// begin. Repeating the same configuration is harmless; changing it after
    /// data has arrived is rejected.
    pub fn configure(&self, spec: AudioSpec) -> Result<()> {
        let byte_budget = spec.queue_byte_budget(AUDIO_QUEUE_MAX_LATENCY)?;
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match state.max_queued_bytes {
            Some(existing) if existing != byte_budget => {
                bail!("Audio capture changed format after its queue was configured.")
            }
            Some(_) => {}
            None if state.chunks.is_empty() => state.max_queued_bytes = Some(byte_budget),
            None => bail!("Audio data arrived before its queue was configured."),
        }
        Ok(())
    }

    /// Non-blocking send for real-time/native callbacks.
    pub fn try_send(&self, chunk: Vec<u8>) -> std::result::Result<(), AudioQueueSendError> {
        if chunk.is_empty() {
            return Ok(());
        }
        let payload_bytes = chunk.len();
        // Native producers allocate exact-sized chunks in normal operation.
        // Accounting capacity makes an accidentally over-allocated Vec unable
        // to bypass the memory budget.
        let allocated_bytes = chunk.capacity().max(payload_bytes);
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !state.receiver_open {
            return Err(AudioQueueSendError::Closed);
        }
        let Some(max_queued_bytes) = state.max_queued_bytes else {
            record_audio_drop(&mut state, payload_bytes);
            return Err(AudioQueueSendError::Unconfigured);
        };
        let exceeds_bytes = allocated_bytes > max_queued_bytes.saturating_sub(state.queued_bytes);
        if exceeds_bytes || state.chunks.len() >= state.max_chunks {
            record_audio_drop(&mut state, payload_bytes);
            return Err(AudioQueueSendError::Overloaded);
        }
        state.queued_bytes += allocated_bytes;
        let enqueued_at = state.consumer_attached.then(Instant::now);
        state.chunks.push_back(QueuedAudioChunk {
            bytes: chunk,
            enqueued_at,
        });
        self.shared.ready.notify_one();
        Ok(())
    }

    /// Records a native-device failure without allocating or retaining the
    /// platform error text on a real-time callback.
    #[cfg(any(windows, test))]
    pub fn report_capture_failure(&self) {
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.capture_failed = true;
    }
}

impl Drop for AudioChunkSender {
    fn drop(&mut self) {
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.sender_count = state.sender_count.saturating_sub(1);
        if state.sender_count == 0 {
            self.shared.ready.notify_all();
        }
    }
}

pub struct AudioChunkReceiver {
    shared: Arc<AudioQueueShared>,
}

enum AudioQueueReceive {
    Chunk(QueuedAudioChunk),
    Closed,
    TimedOut,
}

impl AudioChunkReceiver {
    fn recv_queued(&self) -> Option<QueuedAudioChunk> {
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        loop {
            if let Some(chunk) = state.chunks.pop_front() {
                state.queued_bytes = state.queued_bytes.saturating_sub(chunk.bytes.capacity());
                return Some(chunk);
            }
            if state.sender_count == 0 {
                return None;
            }
            state = self
                .shared
                .ready
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }

    fn recv(&self) -> Option<Vec<u8>> {
        self.recv_queued().map(|chunk| chunk.bytes)
    }

    fn recv_queued_timeout(&self, timeout: Duration) -> AudioQueueReceive {
        let deadline = Instant::now() + timeout;
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        loop {
            if let Some(chunk) = state.chunks.pop_front() {
                state.queued_bytes = state.queued_bytes.saturating_sub(chunk.bytes.capacity());
                return AudioQueueReceive::Chunk(chunk);
            }
            if state.sender_count == 0 {
                return AudioQueueReceive::Closed;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return AudioQueueReceive::TimedOut;
            }
            let (next_state, wait_result) = self
                .shared
                .ready
                .wait_timeout(state, remaining)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state = next_state;
            if wait_result.timed_out() && state.chunks.is_empty() {
                return AudioQueueReceive::TimedOut;
            }
        }
    }

    /// Makes the encoder hand-off the recording timeline boundary. Native
    /// streams start first so their format can configure the encoder; discard that
    /// short pre-roll and its capacity counters atomically so it cannot create
    /// either startup drift or a false overload failure.
    fn attach_consumer(&self) {
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.consumer_attached {
            return;
        }
        state.chunks.clear();
        state.queued_bytes = 0;
        state.dropped_chunks = 0;
        state.dropped_bytes = 0;
        state.consumer_attached = true;
    }

    fn report_handoff_residence(&self, residence: Duration) {
        if residence <= AUDIO_QUEUE_MAX_RESIDENCE {
            return;
        }
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.late_chunks = state.late_chunks.saturating_add(1);
        state.max_residence = state.max_residence.max(residence);
    }

    fn report_writer_failure(&self) {
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.writer_failed = true;
    }

    fn status(&self) -> AudioQueueStatus {
        AudioQueueStatus {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl Iterator for AudioChunkReceiver {
    type Item = Vec<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        self.recv()
    }
}

impl Drop for AudioChunkReceiver {
    fn drop(&mut self) {
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.receiver_open = false;
        state.chunks.clear();
        state.queued_bytes = 0;
        self.shared.ready.notify_all();
    }
}

pub fn bounded_audio_channel() -> (AudioChunkSender, AudioChunkReceiver) {
    audio_channel_with_limits(None, AUDIO_QUEUE_MAX_CHUNKS)
}

fn audio_channel_with_limits(
    max_queued_bytes: Option<usize>,
    max_chunks: usize,
) -> (AudioChunkSender, AudioChunkReceiver) {
    let shared = Arc::new(AudioQueueShared {
        state: Mutex::new(AudioQueueState {
            chunks: VecDeque::new(),
            queued_bytes: 0,
            max_queued_bytes,
            max_chunks,
            sender_count: 1,
            receiver_open: true,
            consumer_attached: false,
            dropped_chunks: 0,
            dropped_bytes: 0,
            late_chunks: 0,
            max_residence: Duration::ZERO,
            capture_failed: false,
            writer_failed: false,
        }),
        ready: Condvar::new(),
    });
    (
        AudioChunkSender {
            shared: Arc::clone(&shared),
        },
        AudioChunkReceiver { shared },
    )
}

fn record_audio_drop(state: &mut AudioQueueState, bytes: usize) {
    state.dropped_chunks = state.dropped_chunks.saturating_add(1);
    state.dropped_bytes = state
        .dropped_bytes
        .saturating_add(u64::try_from(bytes).unwrap_or(u64::MAX));
}

#[derive(Debug, Clone)]
pub struct EncoderConfig {
    pub width: i64,
    pub height: i64,
    pub fps: u32,
    pub bitrate: i64,
    /// Optional system-audio capture format.
    pub audio: Option<AudioSpec>,
    /// Optional microphone capture format.
    pub mic: Option<AudioSpec>,
}

pub fn bitrate_for(width: i64, height: i64) -> i64 {
    RecordingPolicy::high_quality_bit_rate(width, height)
}

#[cfg(target_os = "macos")]
pub fn probe_video_native(video: &Path) -> Option<(i64, i64, Option<f64>)> {
    crate::macos_media::probe_media(video).ok()
}

#[cfg(target_os = "macos")]
pub fn merge_segments_native(segments: &[PathBuf], out_path: &Path) -> Result<()> {
    crate::macos_media::merge_segments(segments, out_path)
}

// ---------------------------------------------------------------------------
// Segment encoder
// ---------------------------------------------------------------------------

/// A running encoder segment writing to `out_path`.
pub struct SegmentEncoder {
    inner: SegmentEncoderInner,
}

enum SegmentEncoderInner {
    #[cfg(target_os = "macos")]
    MacosNative(MacosNativeSegmentEncoder),
    #[cfg(windows)]
    WindowsNative(WindowsNativeSegmentEncoder),
}

const ENCODER_INPUT_POLL_INTERVAL: Duration = Duration::from_millis(25);
const NATIVE_ENCODER_INPUT_QUEUE_CAPACITY: usize = 2;

fn ensure_audio_queues_healthy(statuses: &[AudioQueueStatus]) -> Result<()> {
    let dropped = statuses
        .iter()
        .fold(AudioQueueSnapshot::default(), |mut total, status| {
            let snapshot = status.snapshot();
            total.dropped_chunks = total.dropped_chunks.saturating_add(snapshot.dropped_chunks);
            total.dropped_bytes = total.dropped_bytes.saturating_add(snapshot.dropped_bytes);
            total.late_chunks = total.late_chunks.saturating_add(snapshot.late_chunks);
            total.max_residence = total.max_residence.max(snapshot.max_residence);
            total.capture_failed |= snapshot.capture_failed;
            total.writer_failed |= snapshot.writer_failed;
            total
        });
    if dropped.dropped_chunks > 0
        || dropped.late_chunks > 0
        || dropped.capture_failed
        || dropped.writer_failed
    {
        bail!(
            "Audio capture lost integrity ({} dropped chunks / {} bytes, {} delayed chunks, max hand-off {} ms, capture fault={}, encoder fault={}); the recording was not saved to avoid A/V desynchronization.",
            dropped.dropped_chunks,
            dropped.dropped_bytes,
            dropped.late_chunks,
            dropped.max_residence.as_millis(),
            dropped.capture_failed,
            dropped.writer_failed,
        );
    }
    Ok(())
}

#[cfg(windows)]
enum WindowsNativeInput {
    Video(Vec<u8>),
    Audio(usize, Vec<u8>),
}

#[cfg(windows)]
struct WindowsNativeSegmentEncoder {
    out_path: PathBuf,
    inputs: Vec<JoinHandle<()>>,
    worker: JoinHandle<Result<()>>,
    audio_statuses: Vec<AudioQueueStatus>,
    shutdown: Arc<AtomicBool>,
}

#[cfg(windows)]
impl WindowsNativeSegmentEncoder {
    fn start(
        config: &EncoderConfig,
        out_path: PathBuf,
        video_rx: mpsc::Receiver<Vec<u8>>,
        audio_rx: Option<AudioChunkReceiver>,
        mic_rx: Option<AudioChunkReceiver>,
    ) -> Result<Self> {
        use windows_capture::encoder::{
            AudioSettingsBuilder, ContainerSettingsBuilder, VideoEncoder, VideoSettingsBuilder,
            VideoSettingsSubType,
        };

        let width = u32::try_from(config.width).context("invalid native encoder width")?;
        let height = u32::try_from(config.height).context("invalid native encoder height")?;
        let bitrate = u32::try_from(config.bitrate).context("invalid native encoder bitrate")?;
        let audio_enabled = audio_rx.is_some() || mic_rx.is_some();
        let encoder = VideoEncoder::new(
            VideoSettingsBuilder::new(width, height)
                .bitrate(bitrate)
                .frame_rate(config.fps)
                .sub_type(VideoSettingsSubType::H264),
            AudioSettingsBuilder::new().disabled(!audio_enabled),
            ContainerSettingsBuilder::default(),
            &out_path,
        )
        .context("could not initialize the Windows H.264 encoder")?;

        let (input_tx, input_rx) = mpsc::sync_channel(NATIVE_ENCODER_INPUT_QUEUE_CAPACITY);
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker_shutdown = Arc::clone(&shutdown);
        let fps = config.fps;
        let system_spec = config.audio;
        let microphone_spec = config.mic;
        let worker = std::thread::spawn(move || -> Result<()> {
            let mut encoder = encoder;
            let mut frame_index = 0i64;
            let mut mixer = NativeAudioMixer::new(system_spec.is_some(), microphone_spec.is_some());
            while let Ok(input) = input_rx.recv() {
                if worker_shutdown.load(Ordering::Acquire) {
                    break;
                }
                match input {
                    WindowsNativeInput::Video(mut frame) => {
                        flip_bgra_rows(&mut frame, width as usize, height as usize)?;
                        let timestamp = frame_index.saturating_mul(10_000_000) / i64::from(fps);
                        encoder
                            .send_frame_buffer(&frame, timestamp)
                            .context("Windows H.264 encoder rejected a video frame")?;
                        frame_index = frame_index.saturating_add(1);
                    }
                    WindowsNativeInput::Audio(index, bytes) => {
                        let spec = if index == 0 {
                            system_spec
                        } else {
                            microphone_spec
                        };
                        if let Some(spec) = spec {
                            mixer
                                .push(index, pcm_to_48khz_stereo_i16(&bytes, spec))
                                .context("native audio mixer exceeded its bounded buffer")?;
                            for block in mixer.drain(false) {
                                encoder
                                    .send_audio_buffer(&block, 0)
                                    .context("Windows H.264 encoder rejected an audio buffer")?;
                            }
                        }
                    }
                }
            }
            if worker_shutdown.load(Ordering::Acquire) {
                drop(encoder);
                return Ok(());
            }
            for block in mixer.drain(true) {
                encoder
                    .send_audio_buffer(&block, 0)
                    .context("Windows H.264 encoder rejected the final audio buffer")?;
            }
            encoder
                .finish()
                .context("Windows H.264 encoder could not finalize the MP4")
        });

        let mut inputs = Vec::new();
        let video_tx = input_tx.clone();
        let video_shutdown = Arc::clone(&shutdown);
        inputs.push(std::thread::spawn(move || loop {
            if video_shutdown.load(Ordering::Acquire) {
                break;
            }
            match video_rx.recv_timeout(ENCODER_INPUT_POLL_INTERVAL) {
                Ok(frame) => {
                    if video_tx.send(WindowsNativeInput::Video(frame)).is_err() {
                        break;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }));

        let mut audio_statuses = Vec::new();
        for (index, receiver) in [audio_rx, mic_rx].into_iter().enumerate() {
            let Some(receiver) = receiver else { continue };
            receiver.attach_consumer();
            audio_statuses.push(receiver.status());
            let audio_tx = input_tx.clone();
            let audio_shutdown = Arc::clone(&shutdown);
            inputs.push(std::thread::spawn(move || loop {
                if audio_shutdown.load(Ordering::Acquire) {
                    break;
                }
                let chunk = match receiver.recv_queued_timeout(ENCODER_INPUT_POLL_INTERVAL) {
                    AudioQueueReceive::Chunk(chunk) => chunk,
                    AudioQueueReceive::Closed => break,
                    AudioQueueReceive::TimedOut => continue,
                };
                if let Some(enqueued_at) = chunk.enqueued_at {
                    receiver.report_handoff_residence(enqueued_at.elapsed());
                }
                if audio_tx
                    .send(WindowsNativeInput::Audio(index, chunk.bytes))
                    .is_err()
                {
                    receiver.report_writer_failure();
                    break;
                }
            }));
        }
        drop(input_tx);

        Ok(Self {
            out_path,
            inputs,
            worker,
            audio_statuses,
            shutdown,
        })
    }

    fn join_inputs(&mut self) {
        for status in &self.audio_statuses {
            status.shared.ready.notify_all();
        }
        for input in self.inputs.drain(..) {
            if input.join().is_err() {
                log::error!("record: native encoder input thread panicked during shutdown");
            }
        }
    }

    fn finish(mut self) -> Result<PathBuf> {
        self.join_inputs();
        let worker_result = self
            .worker
            .join()
            .map_err(|_| anyhow::anyhow!("Windows H.264 encoder thread panicked"))?;
        if let Err(error) = worker_result {
            let _ = std::fs::remove_file(&self.out_path);
            return Err(error);
        }
        if let Err(error) = ensure_audio_queues_healthy(&self.audio_statuses) {
            let _ = std::fs::remove_file(&self.out_path);
            return Err(error);
        }
        Ok(self.out_path)
    }

    fn cancel(mut self) {
        self.shutdown.store(true, Ordering::Release);
        self.join_inputs();
        let _ = self.worker.join();
        let _ = std::fs::remove_file(&self.out_path);
    }
}

#[cfg(target_os = "macos")]
enum MacosNativeInput {
    Video(Vec<u8>),
    Audio(usize, Vec<u8>),
}

#[cfg(target_os = "macos")]
struct MacosNativeSegmentEncoder {
    out_path: PathBuf,
    inputs: Vec<JoinHandle<()>>,
    worker: JoinHandle<Result<()>>,
    audio_statuses: Vec<AudioQueueStatus>,
    shutdown: Arc<AtomicBool>,
}

#[cfg(target_os = "macos")]
impl MacosNativeSegmentEncoder {
    fn start(
        config: &EncoderConfig,
        out_path: PathBuf,
        video_rx: mpsc::Receiver<Vec<u8>>,
        audio_rx: Option<AudioChunkReceiver>,
        mic_rx: Option<AudioChunkReceiver>,
    ) -> Result<Self> {
        let width = u32::try_from(config.width).context("invalid native encoder width")?;
        let height = u32::try_from(config.height).context("invalid native encoder height")?;
        let audio_enabled = audio_rx.is_some() || mic_rx.is_some();
        let encoder = crate::macos_media::MacosSegmentEncoder::new(
            &out_path,
            width,
            height,
            config.fps,
            config.bitrate,
            audio_enabled,
        )?;

        let (input_tx, input_rx) = mpsc::sync_channel(NATIVE_ENCODER_INPUT_QUEUE_CAPACITY);
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker_shutdown = Arc::clone(&shutdown);
        let system_spec = config.audio;
        let microphone_spec = config.mic;
        let worker = std::thread::spawn(move || -> Result<()> {
            let mut encoder = encoder;
            let mut frame_index = 0i64;
            let mut mixer = NativeAudioMixer::new(system_spec.is_some(), microphone_spec.is_some());
            while let Ok(input) = input_rx.recv() {
                if worker_shutdown.load(Ordering::Acquire) {
                    encoder.cancel();
                    return Ok(());
                }
                match input {
                    MacosNativeInput::Video(frame) => {
                        if !encoder
                            .append_video(&frame, frame_index)
                            .context("macOS H.264 encoder rejected a video frame")?
                        {
                            log::warn!(
                                "record: macOS encoder dropped a video frame under backpressure"
                            );
                        }
                        frame_index = frame_index.saturating_add(1);
                    }
                    MacosNativeInput::Audio(index, bytes) => {
                        let spec = if index == 0 {
                            system_spec
                        } else {
                            microphone_spec
                        };
                        if let Some(spec) = spec {
                            mixer
                                .push(index, pcm_to_48khz_stereo_i16(&bytes, spec))
                                .context("native audio mixer exceeded its bounded buffer")?;
                            for block in mixer.drain(false) {
                                encoder
                                    .append_audio(&block)
                                    .context("macOS AAC encoder rejected an audio buffer")?;
                            }
                        }
                    }
                }
            }
            if worker_shutdown.load(Ordering::Acquire) {
                encoder.cancel();
                return Ok(());
            }
            for block in mixer.drain(true) {
                encoder
                    .append_audio(&block)
                    .context("macOS AAC encoder rejected the final audio buffer")?;
            }
            encoder
                .finish()
                .context("macOS native encoder could not finalize the MP4")
        });

        let mut inputs = Vec::new();
        let video_tx = input_tx.clone();
        let video_shutdown = Arc::clone(&shutdown);
        inputs.push(std::thread::spawn(move || loop {
            if video_shutdown.load(Ordering::Acquire) {
                break;
            }
            match video_rx.recv_timeout(ENCODER_INPUT_POLL_INTERVAL) {
                Ok(frame) => {
                    if video_tx.send(MacosNativeInput::Video(frame)).is_err() {
                        break;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }));

        let mut audio_statuses = Vec::new();
        for (index, receiver) in [audio_rx, mic_rx].into_iter().enumerate() {
            let Some(receiver) = receiver else { continue };
            receiver.attach_consumer();
            audio_statuses.push(receiver.status());
            let audio_tx = input_tx.clone();
            let audio_shutdown = Arc::clone(&shutdown);
            inputs.push(std::thread::spawn(move || loop {
                if audio_shutdown.load(Ordering::Acquire) {
                    break;
                }
                let chunk = match receiver.recv_queued_timeout(ENCODER_INPUT_POLL_INTERVAL) {
                    AudioQueueReceive::Chunk(chunk) => chunk,
                    AudioQueueReceive::Closed => break,
                    AudioQueueReceive::TimedOut => continue,
                };
                if let Some(enqueued_at) = chunk.enqueued_at {
                    receiver.report_handoff_residence(enqueued_at.elapsed());
                }
                if audio_tx
                    .send(MacosNativeInput::Audio(index, chunk.bytes))
                    .is_err()
                {
                    receiver.report_writer_failure();
                    break;
                }
            }));
        }
        drop(input_tx);

        Ok(Self {
            out_path,
            inputs,
            worker,
            audio_statuses,
            shutdown,
        })
    }

    fn join_inputs(&mut self) {
        for status in &self.audio_statuses {
            status.shared.ready.notify_all();
        }
        for input in self.inputs.drain(..) {
            if input.join().is_err() {
                log::error!("record: macOS native encoder input thread panicked during shutdown");
            }
        }
    }

    fn finish(mut self) -> Result<PathBuf> {
        self.join_inputs();
        let worker_result = self
            .worker
            .join()
            .map_err(|_| anyhow::anyhow!("macOS native encoder thread panicked"))?;
        if let Err(error) = worker_result {
            let _ = std::fs::remove_file(&self.out_path);
            return Err(error);
        }
        if let Err(error) = ensure_audio_queues_healthy(&self.audio_statuses) {
            let _ = std::fs::remove_file(&self.out_path);
            return Err(error);
        }
        Ok(self.out_path)
    }

    fn cancel(mut self) {
        self.shutdown.store(true, Ordering::Release);
        self.join_inputs();
        let _ = self.worker.join();
        let _ = std::fs::remove_file(&self.out_path);
    }
}

#[cfg(windows)]
fn flip_bgra_rows(frame: &mut [u8], width: usize, height: usize) -> Result<()> {
    let row_bytes = width
        .checked_mul(4)
        .context("native video row is too wide")?;
    let expected = row_bytes
        .checked_mul(height)
        .context("native video frame is too large")?;
    if frame.len() != expected {
        bail!(
            "native video frame has {} bytes; expected {expected}",
            frame.len()
        );
    }
    for top in 0..height / 2 {
        let bottom = height - 1 - top;
        let split = bottom * row_bytes;
        let (before_bottom, bottom_and_after) = frame.split_at_mut(split);
        before_bottom[top * row_bytes..(top + 1) * row_bytes]
            .swap_with_slice(&mut bottom_and_after[..row_bytes]);
    }
    Ok(())
}

#[cfg(any(windows, target_os = "macos"))]
fn pcm_to_48khz_stereo_i16(bytes: &[u8], spec: AudioSpec) -> Vec<i16> {
    let bytes_per_sample = spec.format.bytes_per_sample() as usize;
    let channels = spec.channels.max(1) as usize;
    let input_frames = bytes.len() / (bytes_per_sample * channels);
    if input_frames == 0 || spec.sample_rate == 0 {
        return Vec::new();
    }
    let output_frames =
        ((input_frames as u64 * 48_000) / u64::from(spec.sample_rate)).max(1) as usize;
    let mut output = Vec::with_capacity(output_frames * 2);
    for output_frame in 0..output_frames {
        let input_frame = ((output_frame as u64 * u64::from(spec.sample_rate)) / 48_000)
            .min(input_frames.saturating_sub(1) as u64) as usize;
        let sample = |channel: usize| {
            let channel = channel.min(channels - 1);
            let offset = (input_frame * channels + channel) * bytes_per_sample;
            match spec.format {
                AudioSampleFormat::F32 => {
                    let value = f32::from_ne_bytes(bytes[offset..offset + 4].try_into().unwrap());
                    (value.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16
                }
                #[cfg(windows)]
                AudioSampleFormat::I16 => {
                    i16::from_ne_bytes(bytes[offset..offset + 2].try_into().unwrap())
                }
                #[cfg(windows)]
                AudioSampleFormat::U16 => {
                    let value = u16::from_ne_bytes(bytes[offset..offset + 2].try_into().unwrap());
                    (i32::from(value) - 32_768) as i16
                }
            }
        };
        let left = sample(0);
        let right = if channels == 1 { left } else { sample(1) };
        output.extend([left, right]);
    }
    output
}

#[cfg(any(windows, target_os = "macos"))]
struct NativeAudioMixer {
    active: [bool; 2],
    samples: [VecDeque<i16>; 2],
}

#[cfg(any(windows, target_os = "macos"))]
impl NativeAudioMixer {
    const BLOCK_SAMPLES: usize = 480 * 2;
    const MAX_BUFFERED_SAMPLES_PER_INPUT: usize = 48_000 * 2 / 4;

    fn new(system: bool, microphone: bool) -> Self {
        Self {
            active: [system, microphone],
            samples: [VecDeque::new(), VecDeque::new()],
        }
    }

    fn push(&mut self, index: usize, samples: Vec<i16>) -> Result<()> {
        if index >= self.samples.len()
            || self.samples[index].len().saturating_add(samples.len())
                > Self::MAX_BUFFERED_SAMPLES_PER_INPUT
        {
            bail!("native audio inputs drifted more than 250 ms apart");
        }
        self.samples[index].extend(samples);
        Ok(())
    }

    fn drain(&mut self, force: bool) -> Vec<Vec<u8>> {
        let mut blocks = Vec::new();
        loop {
            let available = if self.active[0] && self.active[1] {
                if force {
                    self.samples[0].len().max(self.samples[1].len())
                } else {
                    self.samples[0].len().min(self.samples[1].len())
                }
            } else if self.active[0] {
                self.samples[0].len()
            } else if self.active[1] {
                self.samples[1].len()
            } else {
                0
            };
            if available == 0 || (!force && available < Self::BLOCK_SAMPLES) {
                break;
            }
            let count = available.min(Self::BLOCK_SAMPLES);
            let mut bytes = Vec::with_capacity(count * 2);
            for _ in 0..count {
                let system = self.samples[0].pop_front().unwrap_or(0) as i32;
                let microphone = self.samples[1].pop_front().unwrap_or(0) as i32;
                let mixed = match (self.active[0], self.active[1]) {
                    (true, true) => system.saturating_add(microphone),
                    (true, false) => system,
                    (false, true) => microphone,
                    (false, false) => 0,
                }
                .clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
                bytes.extend_from_slice(&mixed.to_le_bytes());
            }
            blocks.push(bytes);
        }
        blocks
    }
}

impl SegmentEncoder {
    #[cfg(windows)]
    pub fn start_windows_native(
        config: &EncoderConfig,
        out_path: PathBuf,
        video_rx: mpsc::Receiver<Vec<u8>>,
        audio_rx: Option<AudioChunkReceiver>,
        mic_rx: Option<AudioChunkReceiver>,
    ) -> Result<Self> {
        Ok(Self {
            inner: SegmentEncoderInner::WindowsNative(WindowsNativeSegmentEncoder::start(
                config, out_path, video_rx, audio_rx, mic_rx,
            )?),
        })
    }

    #[cfg(target_os = "macos")]
    pub fn start_macos_native(
        config: &EncoderConfig,
        out_path: PathBuf,
        video_rx: mpsc::Receiver<Vec<u8>>,
        audio_rx: Option<AudioChunkReceiver>,
        mic_rx: Option<AudioChunkReceiver>,
    ) -> Result<Self> {
        Ok(Self {
            inner: SegmentEncoderInner::MacosNative(MacosNativeSegmentEncoder::start(
                config, out_path, video_rx, audio_rx, mic_rx,
            )?),
        })
    }

    pub fn is_windows_native(&self) -> bool {
        #[cfg(windows)]
        {
            matches!(self.inner, SegmentEncoderInner::WindowsNative(_))
        }
        #[cfg(not(windows))]
        {
            false
        }
    }

    pub fn finish(self) -> Result<PathBuf> {
        match self.inner {
            #[cfg(target_os = "macos")]
            SegmentEncoderInner::MacosNative(encoder) => encoder.finish(),
            #[cfg(windows)]
            SegmentEncoderInner::WindowsNative(encoder) => encoder.finish(),
        }
    }

    pub fn cancel(self) {
        match self.inner {
            #[cfg(target_os = "macos")]
            SegmentEncoderInner::MacosNative(encoder) => encoder.cancel(),
            #[cfg(windows)]
            SegmentEncoderInner::WindowsNative(encoder) => encoder.cancel(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_queue_budget_is_derived_from_pcm_time() {
        let spec = AudioSpec {
            sample_rate: 48_000,
            channels: 2,
            format: AudioSampleFormat::F32,
        };
        assert_eq!(
            spec.queue_byte_budget(AUDIO_QUEUE_MAX_LATENCY).unwrap(),
            96_000
        );
    }

    #[test]
    fn audio_queue_rejects_overload_without_exceeding_its_byte_bound() {
        let (tx, rx) = audio_channel_with_limits(Some(8), 8);
        let status = rx.status();
        tx.try_send(vec![0; 4]).unwrap();
        tx.try_send(vec![1; 4]).unwrap();

        assert_eq!(
            tx.try_send(vec![2; 1]),
            Err(AudioQueueSendError::Overloaded)
        );
        assert_eq!(
            status.snapshot(),
            AudioQueueSnapshot {
                queued_bytes: 8,
                dropped_chunks: 1,
                dropped_bytes: 1,
                ..AudioQueueSnapshot::default()
            }
        );
    }

    #[test]
    fn audio_queue_receive_releases_capacity() {
        let (tx, rx) = audio_channel_with_limits(Some(4), 1);
        tx.try_send(vec![0; 4]).unwrap();
        assert_eq!(rx.recv().unwrap(), vec![0; 4]);
        tx.try_send(vec![1; 4]).unwrap();
        assert_eq!(rx.recv().unwrap(), vec![1; 4]);
    }

    #[test]
    fn audio_queue_drains_then_closes_after_last_sender() {
        let (tx, rx) = audio_channel_with_limits(Some(4), 1);
        let tx_clone = tx.clone();
        tx.try_send(vec![7; 4]).unwrap();
        drop(tx);
        assert_eq!(rx.recv().unwrap(), vec![7; 4]);
        drop(tx_clone);
        assert!(rx.recv().is_none());
    }

    #[test]
    fn audio_queue_sender_observes_receiver_close() {
        let (tx, rx) = audio_channel_with_limits(Some(4), 1);
        drop(rx);
        assert_eq!(tx.try_send(vec![0; 1]), Err(AudioQueueSendError::Closed));
    }

    #[test]
    fn audio_queue_overload_is_a_segment_integrity_error() {
        let (tx, rx) = audio_channel_with_limits(Some(4), 1);
        rx.attach_consumer();
        let status = rx.status();
        tx.try_send(vec![0; 4]).unwrap();
        assert_eq!(
            tx.try_send(vec![1; 1]),
            Err(AudioQueueSendError::Overloaded)
        );
        let error = ensure_audio_queues_healthy(&[status]).unwrap_err();
        assert!(error.to_string().contains("avoid A/V desynchronization"));
    }

    #[test]
    fn audio_queue_attach_discards_pre_roll_without_a_false_overload() {
        let (tx, rx) = audio_channel_with_limits(Some(4), 1);
        let status = rx.status();
        tx.try_send(vec![0; 4]).unwrap();
        assert_eq!(
            tx.try_send(vec![1; 1]),
            Err(AudioQueueSendError::Overloaded)
        );

        rx.attach_consumer();

        assert_eq!(status.snapshot().queued_bytes, 0);
        ensure_audio_queues_healthy(&[status]).unwrap();
        tx.try_send(vec![2; 4]).unwrap();
        assert_eq!(rx.recv().unwrap(), vec![2; 4]);
    }

    #[test]
    fn delayed_audio_handoff_is_a_segment_integrity_error_without_sleeping() {
        let (_tx, rx) = audio_channel_with_limits(Some(4), 1);
        rx.attach_consumer();
        let status = rx.status();
        rx.report_handoff_residence(AUDIO_QUEUE_MAX_RESIDENCE + Duration::from_nanos(1));

        let error = ensure_audio_queues_healthy(&[status]).unwrap_err();
        assert!(error.to_string().contains("1 delayed chunks"));
    }

    #[test]
    fn capture_and_pipe_faults_are_segment_integrity_errors() {
        let (capture_tx, capture_rx) = audio_channel_with_limits(Some(4), 1);
        let capture_status = capture_rx.status();
        capture_tx.report_capture_failure();
        let capture_error = ensure_audio_queues_healthy(&[capture_status]).unwrap_err();
        assert!(capture_error.to_string().contains("capture fault=true"));

        let (pipe_tx, pipe_rx) = audio_channel_with_limits(Some(4), 1);
        pipe_rx.attach_consumer();
        let pipe_status = pipe_rx.status();
        pipe_tx.try_send(vec![0; 4]).unwrap();
        drop(pipe_tx);
        pipe_rx.report_writer_failure();
        let encoder_error = ensure_audio_queues_healthy(&[pipe_status]).unwrap_err();
        assert!(encoder_error.to_string().contains("encoder fault=true"));
    }

    #[cfg(windows)]
    #[test]
    fn native_frame_buffer_is_flipped_bottom_to_top() {
        let mut frame = vec![
            1, 2, 3, 4, 5, 6, 7, 8, // top row
            9, 10, 11, 12, 13, 14, 15, 16, // bottom row
        ];
        flip_bgra_rows(&mut frame, 2, 2).unwrap();
        assert_eq!(
            frame,
            vec![9, 10, 11, 12, 13, 14, 15, 16, 1, 2, 3, 4, 5, 6, 7, 8]
        );
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn native_audio_converts_mono_float_to_stereo_pcm() {
        let mut input = Vec::new();
        input.extend_from_slice(&0.5f32.to_ne_bytes());
        input.extend_from_slice(&(-0.5f32).to_ne_bytes());
        let output = pcm_to_48khz_stereo_i16(
            &input,
            AudioSpec {
                sample_rate: 48_000,
                channels: 1,
                format: AudioSampleFormat::F32,
            },
        );
        assert_eq!(output.len(), 4);
        assert_eq!(output[0], output[1]);
        assert_eq!(output[2], output[3]);
        assert!(output[0] > 16_000);
        assert!(output[2] < -16_000);
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn native_audio_mixer_combines_system_and_microphone() {
        let mut mixer = NativeAudioMixer::new(true, true);
        mixer
            .push(0, vec![10_000; NativeAudioMixer::BLOCK_SAMPLES])
            .unwrap();
        mixer
            .push(1, vec![5_000; NativeAudioMixer::BLOCK_SAMPLES])
            .unwrap();
        let blocks = mixer.drain(false);
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0]
            .chunks_exact(2)
            .all(|sample| i16::from_le_bytes(sample.try_into().unwrap()) == 15_000));
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn native_audio_mixer_rejects_unbounded_input_skew() {
        let mut mixer = NativeAudioMixer::new(true, true);
        let error = mixer
            .push(
                0,
                vec![0; NativeAudioMixer::MAX_BUFFERED_SAMPLES_PER_INPUT + 1],
            )
            .unwrap_err();
        assert!(error.to_string().contains("more than 250 ms apart"));
    }
}
