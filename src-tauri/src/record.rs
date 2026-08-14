//! Recording encoder — pipes raw video/audio frames into ffmpeg, mirroring
//! the Swift legacy backend's H.264 + AAC + MP4 output.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread::JoinHandle;

use anyhow::{bail, Context, Result};
use once_cell::sync::OnceCell;

use crate::core::policy::RecordingPolicy;

#[derive(Debug, Clone, Copy)]
pub struct AudioSpec {
    pub sample_rate: u32,
    pub channels: u32,
    pub is_float: bool,
}

impl AudioSpec {
    fn ffmpeg_format(&self) -> &'static str {
        if self.is_float {
            "f32le"
        } else {
            "s16le"
        }
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

/// Locates the ffmpeg binary:
/// 1. `KIRI_FFMPEG_PATH` env var
/// 2. bundled resource (production builds)
/// 3. per-user cache (downloaded on first use)
/// 4. `ffmpeg` on PATH
pub fn ffmpeg_binary(resource_dir: Option<PathBuf>) -> PathBuf {
    if let Ok(path) = std::env::var("KIRI_FFMPEG_PATH") {
        if !path.is_empty() {
            return PathBuf::from(path);
        }
    }
    if let Some(resource_dir) = resource_dir {
        let candidate = resource_dir.join("ffmpeg-current").join(binary_name());
        if candidate.exists() {
            return candidate;
        }
    }
    let cached = ffmpeg_cache_path();
    if cached.exists() {
        return cached;
    }
    PathBuf::from("ffmpeg")
}

/// Ensures a usable ffmpeg exists; downloads into the user cache when missing.
pub fn ensure_ffmpeg(resource_dir: Option<PathBuf>) -> Result<PathBuf> {
    let path = ffmpeg_binary(resource_dir);
    if path.as_os_str() != "ffmpeg" && path.exists() {
        return Ok(path);
    }
    if ffmpeg_sidecar::command::ffmpeg_is_installed() {
        return Ok(PathBuf::from("ffmpeg"));
    }
    let cache_path = ffmpeg_cache_path();
    if cache_path.exists() {
        return Ok(cache_path);
    }
    let cache_dir = cache_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&cache_dir)?;
    let url = ffmpeg_sidecar::download::ffmpeg_download_url()
        .context("could not resolve ffmpeg download url")?;
    let archive = ffmpeg_sidecar::download::download_ffmpeg_package(url, &cache_dir)
        .context("could not download ffmpeg")?;
    ffmpeg_sidecar::download::unpack_ffmpeg(&archive, &cache_dir)
        .context("could not unpack ffmpeg")?;
    if let Some(found) = find_file(&cache_dir, binary_name()) {
        if found != cache_path {
            let _ = std::fs::rename(&found, &cache_path);
        }
        return Ok(cache_path);
    }
    bail!("could not locate the unpacked ffmpeg binary")
}

fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_file(&path, name) {
                return Some(found);
            }
        } else if path.file_name().and_then(|n| n.to_str()) == Some(name) {
            return Some(path);
        }
    }
    None
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
    let encoders = encoder_list(binary);
    let candidates: &[&str] = if cfg!(target_os = "macos") {
        &["h264_videotoolbox"]
    } else if cfg!(windows) {
        &["h264_nvenc", "h264_qsv", "h264_amf"]
    } else {
        &["h264_vaapi", "h264_nvenc"]
    };
    for candidate in candidates {
        if encoders
            .lines()
            .any(|line| line.split_whitespace().any(|token| token == *candidate))
        {
            return candidate.to_string();
        }
    }
    "libx264".to_string()
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
        command.args(["-movflags", "+faststart"]).arg(&out_path);

        command.stdin(Stdio::null()).stdout(Stdio::null());
        if mic_pipe.is_some() {
            command.stderr(Stdio::null());
        } else {
            command.stderr(Stdio::piped());
        }

        // Replace stdin/stdout with the OS pipes.
        let (video_rx_pipe, video_tx_pipe) = os_pipe::pipe()?;
        command.stdin(video_rx_pipe);
        if let Some((_audio_rx_pipe, audio_tx_pipe)) = &audio_pipe {
            command.stdout(audio_tx_pipe.try_clone()?);
        }

        let child = command.spawn().context("failed to start ffmpeg")?;
        let mut writers = Vec::new();

        let spawn_writer = |rx: mpsc::Receiver<Vec<u8>>, mut writer: os_pipe::PipeWriter| {
            std::thread::spawn(move || {
                for frame in rx {
                    if writer.write_all(&frame).is_err() {
                        break;
                    }
                }
                let _ = writer.flush();
                // writer dropped → EOF for ffmpeg
            })
        };

        writers.push(spawn_writer(video_rx, video_tx_pipe));
        if let (Some((_audio_rx_pipe, audio_tx_pipe)), Some(rx)) = (&audio_pipe, audio_rx) {
            writers.push(spawn_writer(rx, audio_tx_pipe.try_clone()?));
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
    let hours = hours_part.map(|h| h.parse::<f64>().ok()).unwrap_or(Some(0.0))?;
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
