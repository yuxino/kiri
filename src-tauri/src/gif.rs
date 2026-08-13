//! GIF export via ffmpeg — mirrors GIFExporter.swift parameters.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::core::policy::RecordingPolicy;

/// Exports `video` to a GIF with the original policy:
/// max 15 s, 12 fps, long edge capped at 720 px, infinite loop.
pub fn export_gif(
    video: &Path,
    _resource_dir: Option<PathBuf>,
    max_long_edge: u32,
    fps: u32,
    ffmpeg: &Path,
) -> Result<PathBuf> {
    let out_path = std::env::temp_dir().join(format!(
        "kiri-gif-{}.gif",
        uuid::Uuid::new_v4().to_string().to_lowercase()
    ));

    // palettegen → paletteuse two-pass conversion.
    let palette_path = std::env::temp_dir().join(format!(
        "kiri-gif-palette-{}.png",
        uuid::Uuid::new_v4().to_string().to_lowercase()
    ));

    let filter = format!(
        "fps={fps},scale='min({max_long_edge},iw)':-2:flags=lanczos,split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse"
    );

    let status = std::process::Command::new(ffmpeg)
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-i")
        .arg(video)
        .arg("-t")
        .arg(RecordingPolicy::MAXIMUM_GIF_DURATION.to_string())
        .arg("-filter_complex")
        .arg(filter)
        .arg("-loop")
        .arg("0")
        .arg(&out_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .context("Kiri could not create the GIF file.")?;

    let _ = std::fs::remove_file(&palette_path);

    if !status.success() {
        let _ = std::fs::remove_file(&out_path);
        anyhow::bail!("The GIF could not be finalized.")
    }
    Ok(out_path)
}
