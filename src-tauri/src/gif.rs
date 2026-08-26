//! GIF export via ffmpeg — mirrors GIFExporter.swift parameters.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{bail, Context, Result};

use crate::record::{run_command_with_output_progress, FFMPEG_OUTPUT_STALL_TIMEOUT};

fn gif_filter(max_long_edge: u32, fps: u32) -> String {
    let scale = format!("min(1,{max_long_edge}/max(iw,ih))");
    format!(
        "fps={fps},scale=w='iw*{scale}':h='ih*{scale}':flags=lanczos,split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse"
    )
}

/// Exports a video of any duration to a looping GIF at the requested frame
/// rate, with its long edge capped at `max_long_edge`.
pub fn export_gif(video: &Path, max_long_edge: u32, fps: u32, ffmpeg: &Path) -> Result<PathBuf> {
    export_gif_with_stall_timeout(
        video,
        max_long_edge,
        fps,
        ffmpeg,
        FFMPEG_OUTPUT_STALL_TIMEOUT,
    )
}

fn export_gif_with_stall_timeout(
    video: &Path,
    max_long_edge: u32,
    fps: u32,
    ffmpeg: &Path,
    stall_timeout: Duration,
) -> Result<PathBuf> {
    let out_path = std::env::temp_dir().join(format!(
        "kiri-gif-{}.gif",
        uuid::Uuid::new_v4().to_string().to_lowercase()
    ));

    // palettegen → paletteuse in one filter graph.
    let filter = gif_filter(max_long_edge, fps);

    let status = run_command_with_output_progress(
        Command::new(ffmpeg)
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error")
            .arg("-y")
            .arg("-i")
            .arg(video)
            .arg("-filter_complex")
            .arg(filter)
            .arg("-loop")
            .arg("0")
            .arg(&out_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null()),
        &out_path,
        stall_timeout,
    );

    let status = match status {
        Ok(status) => status,
        Err(error) => {
            let _ = std::fs::remove_file(&out_path);
            return Err(error).context("Kiri could not create the GIF file.");
        }
    };

    if !status.success() {
        let _ = std::fs::remove_file(&out_path);
        bail!("The GIF could not be finalized.")
    }
    Ok(out_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gif_filter_caps_the_long_edge_for_landscape_and_portrait_video() {
        let filter = gif_filter(720, 12);

        // Landscape: max(iw, ih) is iw, so width becomes 720.
        assert!(filter.contains("w='iw*min(1,720/max(iw,ih))'"));
        // Portrait: max(iw, ih) is ih, so height becomes 720.
        assert!(filter.contains("h='ih*min(1,720/max(iw,ih))'"));
    }

    #[cfg(unix)]
    #[test]
    fn gif_export_uses_the_output_progress_watchdog() {
        use std::os::unix::fs::PermissionsExt;
        use std::time::Instant;

        let directory = tempfile::tempdir().unwrap();
        let fake_ffmpeg = directory.path().join("ffmpeg");
        std::fs::write(&fake_ffmpeg, b"#!/bin/sh\nexec /bin/sleep 10\n").unwrap();
        let mut permissions = std::fs::metadata(&fake_ffmpeg).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&fake_ffmpeg, permissions).unwrap();

        let started = Instant::now();
        let error = export_gif_with_stall_timeout(
            Path::new("ignored.mp4"),
            720,
            12,
            &fake_ffmpeg,
            Duration::from_millis(30),
        )
        .unwrap_err();

        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(
            format!("{error:#}").contains("produced no output progress"),
            "unexpected error: {error:#}"
        );
    }
}
