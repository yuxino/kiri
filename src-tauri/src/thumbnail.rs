//! Video/GIF first-frame thumbnails via ffmpeg (never written to disk).

use std::path::Path;

pub fn video_first_frame(ffmpeg: &Path, video: &Path) -> Option<Vec<u8>> {
    let output = std::process::Command::new(ffmpeg)
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-i")
        .arg(video)
        .arg("-frames:v")
        .arg("1")
        .arg("-vf")
        .arg("scale='min(640,iw)':-2")
        .arg("-f")
        .arg("image2pipe")
        .arg("-c:v")
        .arg("png")
        .arg("pipe:1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() || output.stdout.is_empty() {
        return None;
    }
    Some(output.stdout)
}
