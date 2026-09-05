"""Speed up an existing documentation demo without changing its content or app code.

Usage: python retime.py /path/to/repository
Defaults to 2x. Checks provenance before editing and refuses a second retime.
Requires ffmpeg, ffprobe and Pillow; no network or Git writes are performed.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import subprocess
import tempfile
from pathlib import Path

from PIL import Image


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(args: list[str]) -> str:
    result = subprocess.run(args, check=False, capture_output=True, text=True, timeout=180)
    if result.returncode:
        raise RuntimeError(f"{args[0]} failed: {result.stderr[-3000:]}")
    return result.stdout


def probe(path: Path) -> dict:
    return json.loads(run(["ffprobe", "-v", "error", "-show_format", "-show_streams", "-of", "json", str(path)]))


def retime(repo: Path, speed: float = 2.0) -> dict:
    if not math.isfinite(speed) or not 1 < speed <= 3:
        raise ValueError("Speed must be above 1 and at most 3.")
    root = repo.resolve() / "docs" / "demos"
    meta_path = root / "provenance.json"
    meta = json.loads(meta_path.read_text(encoding="utf-8"))
    if "retiming" in meta:
        raise ValueError("Already retimed; use the original recording instead of accelerating twice.")
    before = {name: (root / name).read_bytes() for name in ("demo.mp4", "preview.gif", "poster.png")}
    for name, data in before.items():
        expected = meta["media"][name]
        if len(data) != expected["bytes"] or hashlib.sha256(data).hexdigest() != expected["sha256"]:
            raise ValueError(f"{name}: current file does not match provenance.")
    original_hash = digest(root / "demo.mp4")
    if original_hash != meta["sha256"]:
        raise ValueError("Top-level video checksum does not match.")
    original = probe(root / "demo.mp4")
    streams = original["streams"]
    if len(streams) != 1 or streams[0]["codec_type"] != "video":
        raise ValueError("Only the original silent, single-video demos are supported.")
    old_duration = float(original["format"]["duration"])
    if abs(old_duration - meta["duration"]) > 0.12:
        raise ValueError("Original duration does not match provenance.")
    readme = root / "README.md"
    original_readme = readme.read_text(encoding="utf-8")
    if "<!-- demo-pacing -->" in original_readme:
        raise ValueError("Pacing note already exists.")

    with tempfile.TemporaryDirectory(prefix="demo-retime-", dir=root) as tmp:
        work = Path(tmp)
        video, gif = work / "demo.mp4", work / "preview.gif"
        run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-i", str(root / "demo.mp4"),
             "-map", "0:v:0", "-an", "-vf", f"setpts=(PTS-STARTPTS)/{speed:g},fps=25",
             "-c:v", "libx264", "-preset", "medium", "-crf", "20", "-pix_fmt", "yuv420p",
             "-movflags", "+faststart", "-map_metadata", "-1", "-threads", "2", str(video)])
        filters = ("fps=10,scale=800:-1:flags=lanczos,split[a][b];"
                   "[a]palettegen=max_colors=128[p];"
                   "[b][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle")
        run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-i", str(video),
             "-filter_complex_threads", "1", "-filter_complex", filters,
             "-loop", "0", "-threads", "2", str(gif)])
        result = probe(video)
        duration = float(result["format"]["duration"])
        current = result["streams"][0]
        if len(result["streams"]) != 1 or current["codec_name"] != "h264" or current["pix_fmt"] != "yuv420p":
            raise ValueError("Unexpected output encoding.")
        if (current["width"], current["height"]) != (streams[0]["width"], streams[0]["height"]):
            raise ValueError("Output dimensions changed.")
        if abs(duration - old_duration / speed) > 0.12:
            raise ValueError("Video timing does not match the requested speed.")
        run(["ffmpeg", "-v", "error", "-xerror", "-i", str(video), "-f", "null", "-"])
        with Image.open(gif) as image:
            if not image.is_animated or image.info.get("loop") != 0:
                raise ValueError("Preview is not a looping animation.")
            gif_duration = 0
            for index in range(image.n_frames):
                image.seek(index)
                image.load()
                gif_duration += image.info.get("duration", 0) / 1000
        if abs(gif_duration - duration) > 0.2:
            raise ValueError("Preview timing does not match the video.")
        if max(video.stat().st_size, gif.stat().st_size) > 8_000_000:
            raise ValueError("Output exceeds the documentation size budget.")
        mp4_bytes = video.read_bytes()
        if mp4_bytes.find(b"moov") >= mp4_bytes.find(b"mdat"):
            raise ValueError("MP4 is not optimized for progressive playback.")
        meta["duration"] = duration
        meta["sha256"] = digest(video)
        for name, path in (("demo.mp4", video), ("preview.gif", gif)):
            meta["media"][name] = {"bytes": path.stat().st_size, "sha256": digest(path)}
        meta["retiming"] = {
            "speed": speed, "source_duration": old_duration,
            "source_video_sha256": original_hash,
            "source_preview_sha256": hashlib.sha256(before["preview.gif"]).hexdigest(),
            "tool_sha256": digest(Path(__file__)),
            "video_fps": 25, "gif_fps": 10,
            "method": "Uniform timestamp acceleration; no scene removal, cropping, new UI or audio.",
            "disclosures_preserved": True,
        }
        note = (f"\n\n<!-- demo-pacing -->\nPlayback is paced at **{speed:g}×** the original recording "
                f"({old_duration:.2f}s → {duration:.2f}s). The MP4 and animated preview have matching timing; "
                "all scenes, captions and sample-data disclosures are retained.\n<!-- /demo-pacing -->\n")
        # Finish all checks before replacing anything. Poster and captured-source fields remain unchanged.
        video.replace(root / "demo.mp4")
        gif.replace(root / "preview.gif")
        meta_path.write_text(json.dumps(meta, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        readme.write_text(original_readme.rstrip() + note, encoding="utf-8")
    assert (root / "poster.png").read_bytes() == before["poster.png"]
    return {"project": meta["project"], "before": old_duration, "after": duration, "speed": speed,
            "video_sha256": meta["sha256"], "gif_duration": round(gif_duration, 2)}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("repository", type=Path)
    parser.add_argument("--speed", type=float, default=2.0)
    args = parser.parse_args()
    print(json.dumps(retime(args.repository, args.speed), ensure_ascii=False))
