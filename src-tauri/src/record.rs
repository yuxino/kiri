//! Recording encoder — pipes raw video/audio frames into ffmpeg, mirroring
//! the Swift legacy backend's H.264 + AAC + MP4 output.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{mpsc, Mutex, OnceLock};
use std::thread::JoinHandle;

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

fn validate_ffmpeg(path: &Path) -> Result<()> {
    let status = Command::new(path)
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("the video encoder could not be started")?;
    if !status.success() {
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

static ENCODERS: OnceCell<String> = OnceCell::new();
static VIDEO_ENCODER: OnceCell<String> = OnceCell::new();

fn encoder_list(binary: &Path) -> &str {
    ENCODERS.get_or_init(|| {
        Command::new(binary)
            .arg("-hide_banner")
            .arg("-encoders")
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).to_string())
            .unwrap_or_default()
    })
}

/// Picks a hardware H.264 encoder where available:
/// macOS: h264_videotoolbox; Windows: h264_nvenc → h264_qsv → h264_amf;
/// fallback: libx264.
pub fn pick_video_encoder(binary: &Path) -> String {
    VIDEO_ENCODER
        .get_or_init(|| {
            let encoders = encoder_list(binary);
            let candidates: &[&str] = if cfg!(target_os = "macos") {
                &["h264_videotoolbox"]
            } else if cfg!(windows) {
                &["h264_nvenc", "h264_qsv", "h264_amf"]
            } else {
                &["h264_vaapi", "h264_nvenc"]
            };
            for candidate in candidates {
                let is_listed = encoders
                    .lines()
                    .any(|line| line.split_whitespace().any(|token| token == *candidate));
                // Static Windows builds list NVENC/QSV/AMF even on machines
                // without that hardware. Probe one synthetic frame so Kiri
                // falls back to libx264 before a real recording can be lost.
                if is_listed && video_encoder_is_usable(binary, candidate) {
                    return candidate.to_string();
                }
            }
            "libx264".to_string()
        })
        .clone()
}

fn video_encoder_is_usable(binary: &Path, encoder: &str) -> bool {
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
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
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
}

impl SegmentEncoder {
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        config: &EncoderConfig,
        out_path: PathBuf,
        ffmpeg: &Path,
        video_rx: mpsc::Receiver<Vec<u8>>,
        audio_rx: Option<mpsc::Receiver<Vec<u8>>>,
        mic_rx: Option<mpsc::Receiver<Vec<u8>>>,
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

        let spawn_writer = |rx: mpsc::Receiver<Vec<u8>>, mut writer: os_pipe::PipeWriter| {
            std::thread::spawn(move || {
                let mut total = 0usize;
                let mut chunks = 0usize;
                for frame in rx {
                    if writer.write_all(&frame).is_err() {
                        break;
                    }
                    total += frame.len();
                    chunks += 1;
                }
                log::info!("record: pipe writer flushed {total} bytes in {chunks} chunks");
                let _ = writer.flush();
                // writer dropped → EOF for ffmpeg
            })
        };

        writers.push(spawn_writer(video_rx, video_tx_pipe));
        if let (Some(writer), Some(rx)) = (audio_writer, audio_rx) {
            writers.push(spawn_writer(rx, writer));
        }
        if let (Some(writer), Some(rx)) = (mic_writer, mic_rx) {
            writers.push(spawn_writer(rx, writer));
        }

        Ok(SegmentEncoder {
            child,
            out_path,
            writers,
        })
    }

    /// Closes the pipes, waits for ffmpeg, and returns the finished file.
    pub fn finish(mut self) -> Result<PathBuf> {
        for writer in self.writers.drain(..) {
            let _ = writer.join();
        }
        let status = self.child.wait().context("ffmpeg did not exit")?;
        if !status.success() {
            let _ = std::fs::remove_file(&self.out_path);
            bail!("The MP4 could not be finalized.")
        }
        Ok(self.out_path)
    }

    /// Aborts a segment that cannot be finalized after another recording
    /// component failed. Killing ffmpeg closes its pipe readers; detached
    /// writer threads then exit as their writes fail or their inputs close.
    pub fn cancel(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.out_path);
    }
}

// ---------------------------------------------------------------------------
// Media probing (ffprobe equivalent via `ffmpeg -i` output parsing)
// ---------------------------------------------------------------------------

/// Parses `ffmpeg -i` stderr for video dimensions and duration.
pub fn probe_video(ffmpeg: &Path, video: &Path) -> Option<(i64, i64, Option<f64>)> {
    let output = Command::new(ffmpeg)
        .arg("-hide_banner")
        .arg("-i")
        .arg(video)
        .arg("-f")
        .arg("null")
        .arg("-")
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stderr).to_string();

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

    let copy_status = ffmpeg_command(ffmpeg)
        .args(["-f", "concat", "-safe", "0"])
        .arg("-i")
        .arg(&list_path)
        .args(["-c", "copy"])
        .arg(out_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("concat failed to start")?;

    if copy_status.success() {
        let _ = std::fs::remove_file(&list_path);
        return Ok(());
    }

    let _ = std::fs::remove_file(out_path);
    let status = ffmpeg_command(ffmpeg)
        .args(["-f", "concat", "-safe", "0"])
        .arg("-i")
        .arg(&list_path)
        .args(["-c:v", "libx264", "-c:a", "aac"])
        .args(["-movflags", "+faststart"])
        .arg(out_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
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
        let (audio_tx, audio_rx) = mpsc::channel::<Vec<u8>>();
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

        // 0.5 s of 440 Hz sine, f32le interleaved stereo.
        let rate = 48_000u32;
        let frames = rate / 2;
        let mut audio = Vec::with_capacity(frames as usize * 8);
        for i in 0..frames {
            let v = (0.5f32 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / rate as f32).sin())
                .to_le_bytes();
            audio.extend_from_slice(&v);
            audio.extend_from_slice(&v);
        }
        audio_tx.send(audio).unwrap();
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
