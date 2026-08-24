//! Recording encoder — pipes raw video/audio frames into FFmpeg and produces
//! the application's H.264/HEVC + AAC MP4 output.

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use once_cell::sync::OnceCell;
use sha2::{Digest, Sha256};

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
    fn ffmpeg_format(self) -> &'static str {
        match self {
            Self::F32 => "f32le",
            #[cfg(windows)]
            Self::I16 => "s16le",
            #[cfg(windows)]
            Self::U16 => "u16le",
        }
    }

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
    fn ffmpeg_format(&self) -> &'static str {
        self.format.ffmpeg_format()
    }

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

/// Native audio callbacks must never wait for FFmpeg: doing so can stall the
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
    /// streams start first so their format can configure FFmpeg; discard that
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
    /// System audio (pipe:1).
    pub audio: Option<AudioSpec>,
    /// Microphone (pipe:2, via stderr).
    pub mic: Option<AudioSpec>,
    pub video_encoder: String,
}

// ---------------------------------------------------------------------------
// ffmpeg binary resolution
// ---------------------------------------------------------------------------

fn binary_name() -> &'static str {
    if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    }
}

pub fn ffmpeg_cache_path() -> PathBuf {
    let base = dirs::cache_dir().unwrap_or_else(std::env::temp_dir);
    base.join("kiri").join(binary_name())
}

struct FfmpegArchive {
    url: &'static str,
    sha256: &'static str,
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
const FIXED_FFMPEG_ARCHIVE: Option<FfmpegArchive> = Some(FfmpegArchive {
    url: "https://github.com/GyanD/codexffmpeg/releases/download/9.0.1/ffmpeg-9.0.1-essentials_build.zip",
    sha256: "fec81ae03971d9dd4be3ebe02e263bd2ec1d789483f931bdba5f5715e65da2e9",
});

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
const FIXED_FFMPEG_ARCHIVE: Option<FfmpegArchive> = Some(FfmpegArchive {
    url: "https://ffmpeg.martin-riedl.de/download/macos/amd64/1785871427_9.0/ffmpeg.zip",
    sha256: "79d14663d8b078dbbc38de18d63a30f8a5bfc860af5dfee7f8cf3e387cf1c02c",
});

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const FIXED_FFMPEG_ARCHIVE: Option<FfmpegArchive> = Some(FfmpegArchive {
    url: "https://ffmpeg.martin-riedl.de/download/macos/arm64/1785863997_9.0/ffmpeg.zip",
    sha256: "5267ef149ee0d208057a1b316aac079b661b0476574dee5da7d225769773c603",
});

#[cfg(not(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64")
)))]
const FIXED_FFMPEG_ARCHIVE: Option<FfmpegArchive> = None;

static FFMPEG_INSTALL_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Finds an already installed, validated ffmpeg without network access.
/// Library thumbnails use this path so browsing local captures can never
/// trigger a background download.
pub fn existing_ffmpeg() -> Option<PathBuf> {
    if let Some(path) = std::env::var("KIRI_FFMPEG_PATH")
        .ok()
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
    {
        return validate_ffmpeg(&path).is_ok().then_some(path);
    }
    let cache = ffmpeg_cache_path();
    if validate_ffmpeg(&cache).is_ok() {
        return Some(cache);
    }
    let system = PathBuf::from(binary_name());
    validate_ffmpeg(&system).is_ok().then_some(system)
}

/// Ensures a usable ffmpeg exists; downloads into the user cache when missing.
pub fn ensure_ffmpeg() -> Result<PathBuf> {
    if let Some(path) = std::env::var("KIRI_FFMPEG_PATH")
        .ok()
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
    {
        validate_ffmpeg(&path)
            .context("KIRI_FFMPEG_PATH is not a usable ffmpeg executable; fix or unset it")?;
        return Ok(path);
    }

    // OnceLock caches only a successful path in AppState; it does not make a
    // fallible first installation single-flight. Serialize the cache mutation
    // so simultaneous recording/GIF requests cannot share an archive or
    // extraction directory.
    let _install_guard = FFMPEG_INSTALL_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let cache_path = ffmpeg_cache_path();
    let cache_dir = cache_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(std::env::temp_dir);
    cleanup_unpack_directory(&cache_dir);
    if let Some(source) = FIXED_FFMPEG_ARCHIVE.as_ref() {
        if let Ok(archive) = archive_path(&cache_dir, source) {
            let _ = std::fs::remove_file(archive);
        }
    }
    if cache_path.exists() {
        if validate_ffmpeg(&cache_path).is_ok() {
            return Ok(cache_path);
        }
        log::warn!("record: cached ffmpeg failed validation; replacing it");
        std::fs::remove_file(&cache_path)
            .context("could not replace the unusable cached video encoder")?;
    }
    if validate_ffmpeg(Path::new(binary_name())).is_ok() {
        return Ok(PathBuf::from(binary_name()));
    }

    std::fs::create_dir_all(&cache_dir)?;
    let source = FIXED_FFMPEG_ARCHIVE.as_ref().ok_or_else(|| {
        anyhow::anyhow!("automatic video encoder download is unavailable on this platform")
    })?;
    let expected_archive = archive_path(&cache_dir, source)?;
    let archive = match ffmpeg_sidecar::download::download_ffmpeg_package(source.url, &cache_dir) {
        Ok(path) => path,
        Err(error) => {
            let _ = std::fs::remove_file(expected_archive);
            cleanup_unpack_directory(&cache_dir);
            return Err(error).context("could not download ffmpeg");
        }
    };
    if let Err(error) = verify_archive_sha256(&archive, source.sha256) {
        let _ = std::fs::remove_file(&archive);
        cleanup_unpack_directory(&cache_dir);
        return Err(error);
    }
    let unpack_result =
        ffmpeg_sidecar::download::unpack_ffmpeg_without_extras(&archive, &cache_dir)
            .context("could not unpack ffmpeg");
    let _ = std::fs::remove_file(&archive);
    cleanup_unpack_directory(&cache_dir);
    if let Err(error) = unpack_result {
        let _ = std::fs::remove_file(&cache_path);
        return Err(error);
    }
    if let Err(error) = validate_ffmpeg(&cache_path) {
        let _ = std::fs::remove_file(&cache_path);
        return Err(error).context(
            "the downloaded video encoder is incompatible; try recording again to replace it",
        );
    }
    Ok(cache_path)
}

fn archive_path(cache_dir: &Path, source: &FfmpegArchive) -> Result<PathBuf> {
    let filename = Path::new(source.url)
        .file_name()
        .context("the video encoder download URL has no archive name")?;
    Ok(cache_dir.join(filename))
}

fn cleanup_unpack_directory(cache_dir: &Path) {
    let directory = cache_dir.join(ffmpeg_sidecar::download::UNPACK_DIRNAME);
    if directory.is_dir() {
        let _ = std::fs::remove_dir_all(directory);
    }
}

const ENCODER_PROBE_TIMEOUT: Duration = Duration::from_secs(8);
const CHILD_POLL_INTERVAL: Duration = Duration::from_millis(20);
const PROGRESS_POLL_INTERVAL: Duration = Duration::from_millis(250);
const MERGE_STALL_TIMEOUT: Duration = Duration::from_secs(10 * 60);

#[derive(Debug)]
enum TimedChildExit {
    Exited(ExitStatus),
    TimedOut,
}

fn terminate_and_reap(child: &mut Child) -> Result<()> {
    match child.kill() {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::InvalidInput => {}
        Err(error) => return Err(error).context("could not terminate child process"),
    }
    child.wait().context("could not reap child process")?;
    Ok(())
}

/// Waits without giving an encoder process an unlimited opportunity to hang.
/// Timeout always reaps the child, so callers never leave a zombie or an
/// encoder process holding pipe handles after returning an error.
fn wait_for_child_with_timeout(child: &mut Child, timeout: Duration) -> Result<TimedChildExit> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().context("could not poll child process")? {
            return Ok(TimedChildExit::Exited(status));
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            terminate_and_reap(child).context("could not stop timed-out process")?;
            return Ok(TimedChildExit::TimedOut);
        }
        std::thread::sleep(CHILD_POLL_INTERVAL.min(remaining));
    }
}

/// Long recording merges have no useful fixed total deadline: re-encoding a
/// multi-hour capture can legitimately take hours. Instead, require the output
/// file to keep making byte-level progress. Ten minutes without any size
/// change is treated as a wedged encoder, which is then killed and reaped.
fn wait_for_child_with_output_progress(
    child: &mut Child,
    output_path: &Path,
    stall_timeout: Duration,
) -> Result<TimedChildExit> {
    let mut observed_size = std::fs::metadata(output_path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let mut last_progress = Instant::now();
    loop {
        if let Some(status) = child.try_wait().context("could not poll FFmpeg merge")? {
            return Ok(TimedChildExit::Exited(status));
        }
        let current_size = std::fs::metadata(output_path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        if current_size != observed_size {
            observed_size = current_size;
            last_progress = Instant::now();
        } else if last_progress.elapsed() >= stall_timeout {
            terminate_and_reap(child).context("could not stop stalled FFmpeg merge")?;
            return Ok(TimedChildExit::TimedOut);
        }
        std::thread::sleep(PROGRESS_POLL_INTERVAL.min(stall_timeout));
    }
}

fn run_command_with_output_progress(
    command: &mut Command,
    output_path: &Path,
    stall_timeout: Duration,
) -> Result<ExitStatus> {
    let mut child = command.spawn().context("could not start FFmpeg merge")?;
    let outcome = wait_for_child_with_output_progress(&mut child, output_path, stall_timeout);
    if outcome.is_err() {
        let _ = terminate_and_reap(&mut child);
    }
    match outcome? {
        TimedChildExit::Exited(status) => Ok(status),
        TimedChildExit::TimedOut => bail!(
            "FFmpeg produced no merge output for {} seconds and was terminated.",
            stall_timeout.as_secs()
        ),
    }
}

fn join_output_reader(reader: Option<JoinHandle<std::io::Result<Vec<u8>>>>) -> Result<Vec<u8>> {
    match reader {
        Some(reader) => reader
            .join()
            .map_err(|_| anyhow::anyhow!("child output reader panicked"))?
            .context("could not read child process output"),
        None => Ok(Vec::new()),
    }
}

fn spawn_output_reader<R>(mut reader: Option<R>) -> Option<JoinHandle<std::io::Result<Vec<u8>>>>
where
    R: Read + Send + 'static,
{
    reader.take().map(|mut reader| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes)?;
            Ok(bytes)
        })
    })
}

/// Runs a short-lived FFmpeg/helper command with captured output and a hard
/// deadline. Shared with thumbnail generation so one malformed media file
/// cannot hold its global generation lock forever.
pub(crate) fn run_command_with_timeout(command: &mut Command, timeout: Duration) -> Result<Output> {
    let mut child = command.spawn().context("could not start child process")?;
    let stdout = spawn_output_reader(child.stdout.take());
    let stderr = spawn_output_reader(child.stderr.take());
    let outcome = wait_for_child_with_timeout(&mut child, timeout);
    if outcome.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    let stdout = join_output_reader(stdout)?;
    let stderr = join_output_reader(stderr)?;
    match outcome? {
        TimedChildExit::Exited(status) => Ok(Output {
            status,
            stdout,
            stderr,
        }),
        TimedChildExit::TimedOut => {
            bail!(
                "child process timed out after {} seconds",
                timeout.as_secs_f64()
            )
        }
    }
}

fn validate_ffmpeg(path: &Path) -> Result<()> {
    let output = run_command_with_timeout(
        Command::new(path)
            .arg("-version")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null()),
        ENCODER_PROBE_TIMEOUT,
    )
    .context("the video encoder could not be started")?;
    if !output.status.success() {
        bail!("the video encoder failed its version check");
    }
    Ok(())
}

fn verify_archive_sha256(path: &Path, expected: &str) -> Result<()> {
    let mut file = std::fs::File::open(path).context("could not open the video encoder archive")?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).context("could not verify the video encoder archive")?;
    let actual = format!("{:x}", hasher.finalize());
    if !actual.eq_ignore_ascii_case(expected) {
        bail!("the video encoder archive failed its integrity check");
    }
    Ok(())
}

fn ffmpeg_command(binary: &Path) -> Command {
    let mut command = Command::new(binary);
    command
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-nostdin")
        .arg("-y");
    command
}

static VIDEO_ENCODER: OnceCell<String> = OnceCell::new();

fn encoder_list(binary: &Path) -> Result<String> {
    let output = run_command_with_timeout(
        Command::new(binary)
            .arg("-hide_banner")
            .arg("-encoders")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null()),
        ENCODER_PROBE_TIMEOUT,
    )
    .context("could not inspect FFmpeg video encoders")?;
    if !output.status.success() {
        bail!("FFmpeg failed while listing video encoders.");
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn select_video_encoder<F>(
    encoders: &str,
    hardware_candidates: &[&str],
    mut probe: F,
) -> Result<String>
where
    F: FnMut(&str) -> Result<bool>,
{
    for candidate in hardware_candidates {
        let is_listed = encoders
            .lines()
            .any(|line| line.split_whitespace().any(|token| token == *candidate));
        if !is_listed {
            continue;
        }
        match probe(candidate) {
            Ok(true) => return Ok((*candidate).to_string()),
            Ok(false) => log::warn!("record: listed hardware encoder {candidate} is unusable"),
            Err(error) => log::warn!("record: hardware encoder {candidate} probe failed: {error}"),
        }
    }

    match probe("libx264") {
        Ok(true) => Ok("libx264".to_string()),
        Ok(false) => bail!(
            "FFmpeg does not provide a usable H.264 encoder (hardware probes and libx264 failed)."
        ),
        Err(error) => Err(error).context("the libx264 fallback probe did not complete"),
    }
}

/// Picks a hardware H.264 encoder where available:
/// macOS: h264_videotoolbox; Windows: h264_nvenc → h264_qsv → h264_amf;
/// fallback: libx264.
pub fn pick_video_encoder(binary: &Path) -> Result<String> {
    let encoder = VIDEO_ENCODER.get_or_try_init(|| {
        let encoders = encoder_list(binary)?;
        let candidates: &[&str] = if cfg!(target_os = "macos") {
            &["h264_videotoolbox"]
        } else if cfg!(windows) {
            &["h264_nvenc", "h264_qsv", "h264_amf"]
        } else {
            &["h264_vaapi", "h264_nvenc"]
        };
        // Static Windows builds list NVENC/QSV/AMF even on machines
        // without that hardware. Probe one synthetic frame so Kiri
        // falls back before a real recording can be lost. The software
        // fallback is probed too: its mere presence in a build is not
        // sufficient evidence that it can initialize successfully.
        select_video_encoder(&encoders, candidates, |encoder| {
            video_encoder_is_usable(binary, encoder)
        })
    })?;
    Ok(encoder.clone())
}

fn video_encoder_is_usable(binary: &Path, encoder: &str) -> Result<bool> {
    let output = run_command_with_timeout(
        Command::new(binary)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=black:s=64x64:r=1",
                "-frames:v",
                "1",
                "-an",
                "-c:v",
                encoder,
                "-f",
                "null",
                "-",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null()),
        ENCODER_PROBE_TIMEOUT,
    )?;
    Ok(output.status.success())
}

pub fn bitrate_for(width: i64, height: i64) -> i64 {
    RecordingPolicy::high_quality_bit_rate(width, height)
}

// ---------------------------------------------------------------------------
// Segment encoder
// ---------------------------------------------------------------------------

/// A running encoder segment writing to `out_path`.
pub struct SegmentEncoder {
    child: Child,
    out_path: PathBuf,
    writers: Vec<JoinHandle<()>>,
    audio_statuses: Vec<AudioQueueStatus>,
    writer_shutdown: Arc<AtomicBool>,
}

const PIPE_WRITER_POLL_INTERVAL: Duration = Duration::from_millis(25);
const FFMPEG_FINALIZE_MIN_TIMEOUT: Duration = Duration::from_secs(30);
const FFMPEG_FINALIZE_MAX_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const FFMPEG_FINALIZE_BYTES_PER_SECOND: u64 = 16 * 1024 * 1024;

fn segment_finalize_timeout(out_path: &Path) -> Duration {
    let bytes = std::fs::metadata(out_path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    segment_finalize_timeout_for_bytes(bytes)
}

fn segment_finalize_timeout_for_bytes(bytes: u64) -> Duration {
    let copy_seconds = bytes.div_ceil(FFMPEG_FINALIZE_BYTES_PER_SECOND);
    FFMPEG_FINALIZE_MIN_TIMEOUT
        .saturating_add(Duration::from_secs(copy_seconds))
        .min(FFMPEG_FINALIZE_MAX_TIMEOUT)
}

fn spawn_video_pipe_writer(
    receiver: mpsc::Receiver<Vec<u8>>,
    mut writer: os_pipe::PipeWriter,
    shutdown: Arc<AtomicBool>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut total = 0usize;
        let mut chunk_count = 0usize;
        loop {
            if shutdown.load(Ordering::Acquire) {
                break;
            }
            let chunk = match receiver.recv_timeout(PIPE_WRITER_POLL_INTERVAL) {
                Ok(chunk) => chunk,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            };
            if writer.write_all(&chunk).is_err() {
                break;
            }
            total += chunk.len();
            chunk_count += 1;
        }
        log::info!("record: pipe writer flushed {total} bytes in {chunk_count} chunks");
        let _ = writer.flush();
        // writer dropped → EOF for ffmpeg
    })
}

fn spawn_audio_pipe_writer(
    receiver: AudioChunkReceiver,
    mut writer: os_pipe::PipeWriter,
    shutdown: Arc<AtomicBool>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut total = 0usize;
        let mut chunk_count = 0usize;
        loop {
            if shutdown.load(Ordering::Acquire) {
                break;
            }
            let chunk = match receiver.recv_queued_timeout(PIPE_WRITER_POLL_INTERVAL) {
                AudioQueueReceive::Chunk(chunk) => chunk,
                AudioQueueReceive::Closed => break,
                AudioQueueReceive::TimedOut => continue,
            };
            let bytes = chunk.bytes.len();
            if writer.write_all(&chunk.bytes).is_err() {
                receiver.report_writer_failure();
                break;
            }
            if let Some(enqueued_at) = chunk.enqueued_at {
                receiver.report_handoff_residence(enqueued_at.elapsed());
            }
            total += bytes;
            chunk_count += 1;
        }
        if writer.flush().is_err() {
            receiver.report_writer_failure();
        }
        log::info!("record: audio pipe writer flushed {total} bytes in {chunk_count} chunks");
        // writer dropped → EOF for ffmpeg
    })
}

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
            "Audio capture lost integrity ({} dropped chunks / {} bytes, {} delayed chunks, max hand-off {} ms, capture fault={}, pipe fault={}); the recording was not saved to avoid A/V desynchronization.",
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

impl SegmentEncoder {
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        config: &EncoderConfig,
        out_path: PathBuf,
        ffmpeg: &Path,
        video_rx: mpsc::Receiver<Vec<u8>>,
        audio_rx: Option<AudioChunkReceiver>,
        mic_rx: Option<AudioChunkReceiver>,
    ) -> Result<SegmentEncoder> {
        let mut command = ffmpeg_command(ffmpeg);

        command.args([
            "-f",
            "rawvideo",
            "-pix_fmt",
            "bgra",
            "-s",
            &format!("{}x{}", config.width, config.height),
            "-use_wallclock_as_timestamps",
            "1",
            "-i",
            "pipe:0",
        ]);

        // Audio travels through the child's fd 1 (ffmpeg reads `pipe:1`).
        let audio_pipe = if config.audio.is_some() {
            Some(os_pipe::pipe()?)
        } else {
            None
        };

        // Microphone travels through the child's fd 2 (ffmpeg reads
        // `pipe:2`); error diagnostics are unavailable while it is on.
        let mic_pipe = if config.mic.is_some() {
            Some(os_pipe::pipe()?)
        } else {
            None
        };

        if let Some(audio) = &config.audio {
            command.args([
                "-f",
                audio.ffmpeg_format(),
                "-ar",
                &audio.sample_rate.to_string(),
                "-ac",
                &audio.channels.to_string(),
                "-use_wallclock_as_timestamps",
                "1",
                "-i",
                "pipe:1",
            ]);
        }

        if let Some(mic) = &config.mic {
            command.args([
                "-f",
                mic.ffmpeg_format(),
                "-ar",
                &mic.sample_rate.to_string(),
                "-ac",
                &mic.channels.to_string(),
                "-use_wallclock_as_timestamps",
                "1",
                "-i",
                "pipe:2",
            ]);
        }

        let keyframe_interval = config.fps * 2;
        command.args([
            "-c:v",
            &config.video_encoder,
            "-b:v",
            &config.bitrate.to_string(),
            "-g",
            &keyframe_interval.to_string(),
            "-pix_fmt",
            "yuv420p",
            "-r",
            &config.fps.to_string(),
        ]);
        match (config.audio.is_some(), config.mic.is_some()) {
            (true, true) => {
                // Mix system audio + microphone.
                command.args([
                    "-filter_complex",
                    "[1:a][2:a]amix=inputs=2:duration=longest:normalize=0[aout]",
                    "-map",
                    "0:v",
                    "-map",
                    "[aout]",
                    "-c:a",
                    "aac",
                    "-b:a",
                    "192k",
                ]);
            }
            (true, false) | (false, true) => {
                command.args(["-map", "0:v", "-map", "1:a", "-c:a", "aac", "-b:a", "192k"]);
            }
            (false, false) => {}
        }
        if config.audio.is_some() || config.mic.is_some() {
            // Platform inputs may expose a different native rate/channel
            // layout. The input declarations above describe those bytes
            // exactly; normalize the exported AAC stream to Kiri's 48 kHz
            // stereo recording policy here.
            command.args(["-ar", "48000", "-ac", "2"]);
        }
        command.args(["-movflags", "+faststart"]).arg(&out_path);

        // Replace stdin/stdout with the OS pipes. ffmpeg reads video from its
        // stdin (pipe:0) and audio from its stdout (pipe:1); the writers own
        // the *write* ends of those pipes, ffmpeg gets a *read* end. Wiring a
        // write end into ffmpeg's fd made the audio input fail with EBADF,
        // silently producing MP4s with no audio stream.
        let (video_rx_pipe, video_tx_pipe) = os_pipe::pipe()?;
        command.stdin(video_rx_pipe);
        if let Some((audio_rx_pipe, _audio_tx_pipe)) = &audio_pipe {
            command.stdout(audio_rx_pipe.try_clone()?);
        }
        if let Some((mic_rx_pipe, _mic_tx_pipe)) = &mic_pipe {
            command.stderr(mic_rx_pipe.try_clone()?);
        } else {
            // No caller consumes ffmpeg diagnostics; a piped stderr can fill
            // and deadlock finalization after repeated encoder errors.
            command.stderr(Stdio::null());
        }

        // Complete every fallible pipe clone before starting ffmpeg. Once
        // the child exists, this function must either return its owner or
        // explicitly terminate it; otherwise a clone failure could leave an
        // orphan encoder process behind.
        let audio_writer = audio_pipe
            .as_ref()
            .map(|(_, writer)| writer.try_clone())
            .transpose()?;
        let mic_writer = mic_pipe
            .as_ref()
            .map(|(_, writer)| writer.try_clone())
            .transpose()?;

        let child = command.spawn().context("failed to start ffmpeg")?;
        let mut writers = Vec::new();
        let mut audio_statuses = Vec::new();
        let writer_shutdown = Arc::new(AtomicBool::new(false));

        writers.push(spawn_video_pipe_writer(
            video_rx,
            video_tx_pipe,
            Arc::clone(&writer_shutdown),
        ));
        if let (Some(writer), Some(rx)) = (audio_writer, audio_rx) {
            rx.attach_consumer();
            audio_statuses.push(rx.status());
            writers.push(spawn_audio_pipe_writer(
                rx,
                writer,
                Arc::clone(&writer_shutdown),
            ));
        }
        if let (Some(writer), Some(rx)) = (mic_writer, mic_rx) {
            rx.attach_consumer();
            audio_statuses.push(rx.status());
            writers.push(spawn_audio_pipe_writer(
                rx,
                writer,
                Arc::clone(&writer_shutdown),
            ));
        }

        Ok(SegmentEncoder {
            child,
            out_path,
            writers,
            audio_statuses,
            writer_shutdown,
        })
    }

    fn stop_pipe_writers(&mut self) {
        self.writer_shutdown.store(true, Ordering::Release);
        for status in &self.audio_statuses {
            status.shared.ready.notify_all();
        }
        for writer in self.writers.drain(..) {
            if writer.join().is_err() {
                log::error!("record: pipe writer thread panicked during shutdown");
            }
        }
    }

    /// Closes the pipes, waits for ffmpeg, and returns the finished file.
    pub fn finish(mut self) -> Result<PathBuf> {
        // Writers stream concurrently while ffmpeg drains its inputs. Waiting
        // on them first can deadlock forever if ffmpeg stops reading a full
        // pipe; polling the child first gives the whole finalization one hard
        // deadline. On timeout, killing the child closes every pipe reader,
        // then the cooperative receiver polls let all writer threads exit.
        let finalize_timeout = segment_finalize_timeout(&self.out_path);
        let child_exit = wait_for_child_with_timeout(&mut self.child, finalize_timeout);
        if child_exit.is_err() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        self.stop_pipe_writers();
        let child_exit = child_exit.context("ffmpeg could not be finalized")?;
        let status = match child_exit {
            TimedChildExit::Exited(status) => status,
            TimedChildExit::TimedOut => {
                let _ = std::fs::remove_file(&self.out_path);
                bail!("The MP4 encoder timed out while finalizing and was terminated.");
            }
        };
        if !status.success() {
            let _ = std::fs::remove_file(&self.out_path);
            bail!("The MP4 could not be finalized.")
        }
        if let Err(error) = ensure_audio_queues_healthy(&self.audio_statuses) {
            let _ = std::fs::remove_file(&self.out_path);
            return Err(error);
        }
        Ok(self.out_path)
    }

    /// Aborts a segment that cannot be finalized after another recording
    /// component failed. Killing ffmpeg closes its pipe readers; cooperative
    /// receiver polling then lets every writer thread exit before returning.
    pub fn cancel(mut self) {
        self.writer_shutdown.store(true, Ordering::Release);
        for status in &self.audio_statuses {
            status.shared.ready.notify_all();
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.stop_pipe_writers();
        let _ = std::fs::remove_file(&self.out_path);
    }
}

// ---------------------------------------------------------------------------
// Media probing (ffprobe equivalent via `ffmpeg -i` output parsing)
// ---------------------------------------------------------------------------

/// Parses `ffmpeg -i` stderr for video dimensions and duration.
pub fn probe_video(ffmpeg: &Path, video: &Path) -> Option<(i64, i64, Option<f64>)> {
    // Supplying no output makes FFmpeg stop immediately after opening the
    // input and printing stream metadata. `-f null -` would decode the entire
    // recording, making library import scale with recording duration.
    let output = run_command_with_timeout(
        Command::new(ffmpeg)
            .arg("-hide_banner")
            .arg("-i")
            .arg(video)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped()),
        ENCODER_PROBE_TIMEOUT,
    )
    .ok()?;
    let text = String::from_utf8_lossy(&output.stderr);

    // "Stream #0:0[0x1](und): Video: h264 ... 1920x1080 [SAR ...]" or
    // "... (1920x1080), ..." on some builds.
    let mut width = 0i64;
    let mut height = 0i64;
    for line in text.lines() {
        if !line.contains("Video:") {
            continue;
        }
        if let Some(captured) = find_dimensions(line) {
            width = captured.0;
            height = captured.1;
            break;
        }
    }

    // "Duration: 00:00:12.34, start: ..."
    let mut duration = None;
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("Duration:") else {
            continue;
        };
        let value = rest.split(',').next().unwrap_or("").trim();
        if let Some(seconds) = parse_duration(value) {
            duration = Some(seconds);
        }
        break;
    }

    if width > 0 && height > 0 {
        Some((width, height, duration))
    } else {
        None
    }
}

fn find_dimensions(line: &str) -> Option<(i64, i64)> {
    // Pattern: " 1920x1080 " with optional spaces; or "(1920x1080)".
    for (index, _) in line.match_indices('x') {
        let before = &line[..index];
        let after = &line[index + 1..];
        let width_start = before
            .char_indices()
            .rev()
            .find(|(_, c)| !c.is_ascii_digit())
            .map(|(i, _)| i + 1)
            .unwrap_or(0);
        let height_end = after
            .char_indices()
            .find(|(_, c)| !c.is_ascii_digit())
            .map(|(i, _)| i)
            .unwrap_or(after.len());
        let width_str = &before[width_start..];
        let height_str = &after[..height_end];
        if !width_str.is_empty() && !height_str.is_empty() {
            if let (Ok(w), Ok(h)) = (width_str.parse::<i64>(), height_str.parse::<i64>()) {
                if w >= 16 && h >= 16 && w <= 16_384 && h <= 16_384 {
                    return Some((w, h));
                }
            }
        }
    }
    None
}

fn parse_duration(value: &str) -> Option<f64> {
    // Formats: HH:MM:SS.micro or MM:SS.micro
    let parts: Vec<&str> = value.split(':').collect();
    let (seconds_part, minutes_part, hours_part) = match parts.len() {
        3 => (parts[2], parts[1], Some(parts[0])),
        2 => (parts[1], parts[0], None),
        _ => return None,
    };
    let seconds = seconds_part.parse::<f64>().ok()?;
    let minutes = minutes_part.parse::<f64>().ok()?;
    let hours = hours_part
        .map(|h| h.parse::<f64>().ok())
        .unwrap_or(Some(0.0))?;
    Some(hours * 3600.0 + minutes * 60.0 + seconds)
}

// ---------------------------------------------------------------------------
// Segment merging (RecordingSegmentMerger equivalent)
// ---------------------------------------------------------------------------

/// Merges segments losslessly when possible (concat demuxer with stream
/// copy), falling back to a re-encode.
pub fn merge_segments(segments: &[PathBuf], out_path: &Path, ffmpeg: &Path) -> Result<()> {
    if segments.is_empty() {
        bail!("No recording segments are available.");
    }
    if segments.len() == 1 {
        std::fs::copy(&segments[0], out_path)?;
        return Ok(());
    }

    let list_path = out_path.with_extension("concat.txt");
    let list = segments
        .iter()
        .map(|path| format!("file '{}'", path.display()))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&list_path, list)?;

    let copy_status = run_command_with_output_progress(
        ffmpeg_command(ffmpeg)
            .args(["-f", "concat", "-safe", "0"])
            .arg("-i")
            .arg(&list_path)
            .args(["-c", "copy"])
            .arg(out_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null()),
        out_path,
        MERGE_STALL_TIMEOUT,
    );

    if matches!(&copy_status, Ok(status) if status.success()) {
        let _ = std::fs::remove_file(&list_path);
        return Ok(());
    }
    if let Err(error) = &copy_status {
        log::warn!("record: lossless segment merge failed or stalled: {error}");
    }

    let _ = std::fs::remove_file(out_path);
    let status = run_command_with_output_progress(
        ffmpeg_command(ffmpeg)
            .args(["-f", "concat", "-safe", "0"])
            .arg("-i")
            .arg(&list_path)
            .args(["-c:v", "libx264", "-c:a", "aac"])
            .args(["-movflags", "+faststart"])
            .arg(out_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null()),
        out_path,
        MERGE_STALL_TIMEOUT,
    );
    let _ = std::fs::remove_file(&list_path);
    match status {
        Ok(status) if status.success() => Ok(()),
        _ => bail!("The paused recording could not be merged."),
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
        let (reader, writer) = os_pipe::pipe().unwrap();
        drop(reader);
        spawn_audio_pipe_writer(pipe_rx, writer, Arc::new(AtomicBool::new(false)))
            .join()
            .unwrap();
        let pipe_error = ensure_audio_queues_healthy(&[pipe_status]).unwrap_err();
        assert!(pipe_error.to_string().contains("pipe fault=true"));
    }

    #[test]
    fn encoder_selection_probes_the_software_fallback() {
        let mut probed = Vec::new();
        let selected = select_video_encoder(" V..... h264_nvenc ", &["h264_nvenc"], |encoder| {
            probed.push(encoder.to_string());
            Ok(encoder == "libx264")
        })
        .unwrap();

        assert_eq!(selected, "libx264");
        assert_eq!(probed, vec!["h264_nvenc", "libx264"]);
    }

    #[test]
    fn encoder_selection_rejects_an_unusable_software_fallback() {
        let error = select_video_encoder("", &[], |_| Ok(false)).unwrap_err();
        assert!(error.to_string().contains("libx264 failed"));
    }

    #[cfg(unix)]
    #[test]
    fn child_timeout_terminates_and_reaps_a_stalled_process() {
        let mut child = Command::new("/bin/sleep").arg("10").spawn().unwrap();
        let started = Instant::now();
        let outcome = wait_for_child_with_timeout(&mut child, Duration::from_millis(30)).unwrap();

        assert!(matches!(outcome, TimedChildExit::TimedOut));
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(
            child.try_wait().unwrap().is_some(),
            "timed-out child is reaped"
        );
    }

    #[cfg(unix)]
    #[test]
    fn merge_watchdog_terminates_a_process_with_no_output_progress() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("never-created.mp4");
        let mut child = Command::new("/bin/sleep").arg("10").spawn().unwrap();
        let started = Instant::now();
        let outcome =
            wait_for_child_with_output_progress(&mut child, &output, Duration::from_millis(30))
                .unwrap();

        assert!(matches!(outcome, TimedChildExit::TimedOut));
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(
            child.try_wait().unwrap().is_some(),
            "stalled child is reaped"
        );
    }

    #[test]
    fn segment_finalize_timeout_scales_with_file_size_and_stays_bounded() {
        assert_eq!(
            segment_finalize_timeout_for_bytes(0),
            FFMPEG_FINALIZE_MIN_TIMEOUT
        );
        assert!(
            segment_finalize_timeout_for_bytes(FFMPEG_FINALIZE_BYTES_PER_SECOND)
                > FFMPEG_FINALIZE_MIN_TIMEOUT
        );
        assert_eq!(
            segment_finalize_timeout_for_bytes(u64::MAX),
            FFMPEG_FINALIZE_MAX_TIMEOUT
        );
    }

    #[cfg(unix)]
    #[test]
    fn video_probe_reads_metadata_without_requesting_a_decode_output() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let fake_ffmpeg = directory.path().join("ffmpeg");
        std::fs::write(
            &fake_ffmpeg,
            b"#!/bin/sh\n\
              [ \"$#\" -eq 3 ] || exit 64\n\
              [ \"$1\" = \"-hide_banner\" ] || exit 65\n\
              [ \"$2\" = \"-i\" ] || exit 66\n\
              echo '  Duration: 01:02:03.50, start: 0.000000, bitrate: 1000 kb/s' >&2\n\
              echo '  Stream #0:0: Video: h264, yuv420p, 1920x1080, 30 fps' >&2\n\
              exit 1\n",
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&fake_ffmpeg).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&fake_ffmpeg, permissions).unwrap();

        let metadata = probe_video(&fake_ffmpeg, Path::new("ignored.mp4")).unwrap();
        assert_eq!(metadata, (1920, 1080, Some(3723.5)));
    }

    #[test]
    fn ffmpeg_validation_rejects_a_missing_executable() {
        let missing =
            std::env::temp_dir().join(format!("kiri-missing-ffmpeg-{}", uuid::Uuid::new_v4()));
        assert!(validate_ffmpeg(&missing).is_err());
    }

    #[test]
    fn archive_hash_verification_rejects_tampering() {
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("ffmpeg.zip");
        std::fs::write(&archive, b"abc").unwrap();

        assert!(verify_archive_sha256(
            &archive,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        )
        .is_ok());
        assert!(verify_archive_sha256(&archive, &"0".repeat(64)).is_err());
    }

    /// Regression test for the silent-recording bug: audio must flow through
    /// ffmpeg's stdout (pipe:1) read end. Previously the pipe *write* end was
    /// wired into ffmpeg's stdout, so reading pipe:1 failed with EBADF and
    /// every recording came out without an audio stream.
    #[test]
    fn encoder_writes_audio_stream() {
        let Some(ffmpeg) = ffmpeg_available() else {
            eprintln!("ffmpeg not found; skipping audio-pipe regression test");
            return;
        };
        let out_path =
            std::env::temp_dir().join(format!("kiri-test-audio-{}.mp4", std::process::id()));
        let config = EncoderConfig {
            width: 64,
            height: 64,
            fps: 30,
            bitrate: 500_000,
            audio: Some(AudioSpec {
                sample_rate: 48_000,
                channels: 2,
                format: AudioSampleFormat::F32,
            }),
            mic: None,
            video_encoder: "libx264".into(),
        };

        let (video_tx, video_rx) = mpsc::channel::<Vec<u8>>();
        let (audio_tx, audio_rx) = bounded_audio_channel();
        audio_tx.configure(config.audio.unwrap()).unwrap();
        let encoder = SegmentEncoder::start(
            &config,
            out_path.clone(),
            &ffmpeg,
            video_rx,
            Some(audio_rx),
            None,
        )
        .expect("encoder should start");
        drop(video_tx);

        // 0.125 s of 440 Hz sine, f32le interleaved stereo. Native capture
        // delivers much smaller chunks than the queue's 250 ms byte budget.
        let rate = 48_000u32;
        let frames = rate / 8;
        let mut audio = Vec::with_capacity(frames as usize * 8);
        for i in 0..frames {
            let v = (0.5f32 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / rate as f32).sin())
                .to_le_bytes();
            audio.extend_from_slice(&v);
            audio.extend_from_slice(&v);
        }
        audio_tx.try_send(audio).unwrap();
        drop(audio_tx);

        let finished = encoder.finish().expect("encoder should finish");
        let streams = probe_streams(&ffmpeg, &finished);
        let _ = std::fs::remove_file(&finished);
        assert!(
            streams.contains("Audio:"),
            "output must contain an audio stream, got: {streams}"
        );
    }

    fn ffmpeg_available() -> Option<PathBuf> {
        let candidates = [
            std::env::var("KIRI_FFMPEG_PATH").ok().map(PathBuf::from),
            Some(ffmpeg_cache_path()),
        ];
        candidates
            .into_iter()
            .flatten()
            .find(|candidate| candidate.exists())
    }

    fn probe_streams(ffmpeg: &Path, video: &Path) -> String {
        let output = Command::new(ffmpeg)
            .arg("-hide_banner")
            .arg("-i")
            .arg(video)
            .arg("-f")
            .arg("null")
            .arg("-")
            .output()
            .expect("ffprobe via ffmpeg -i should run");
        String::from_utf8_lossy(&output.stderr).to_string()
    }
}
