"""Installed Linux app smoke test on Xvfb, using real desktop capture pixels.

Invoke through linux-native.sh. This does not exercise GNOME/Wayland portals,
real audio devices, multiple monitors, or fractional scaling.
"""

import argparse
import io
import json
import os
import re
import shutil
from pathlib import Path
import subprocess
import sys
import time

from PIL import Image, ImageChops, ImageGrab, ImageStat
from linux_desktop_fixture import DesktopFixture, RECORDING_REGION


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--executable", required=True, type=Path)
parser.add_argument("--output", required=True, type=Path)
args = parser.parse_args()
profile = Path(os.environ.get("KIRI_QA_PROFILE", "/missing")).resolve()
if sys.platform != "linux" or not profile.is_dir() or os.environ.get("XDG_SESSION_TYPE") != "x11":
    raise SystemExit("Use linux-native.sh to create an isolated Linux desktop")
for variable in ("HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME", "XDG_RUNTIME_DIR", "TMPDIR"):
    if not Path(os.environ[variable]).resolve().is_relative_to(profile):
        raise SystemExit(f"{variable} must be inside the disposable QA profile")

# Use the desktop's accessibility tree and system media libraries. These are
# QA dependencies only; the installed app receives no test-only switches.
import gi

gi.require_version("Atspi", "2.0")
gi.require_version("Gst", "1.0")
gi.require_version("GstPbutils", "1.0")
gi.require_version("GstVideo", "1.0")
from gi.repository import Atspi, Gio, GLib, Gst, GstPbutils, GstVideo

output = args.output.resolve()
output.mkdir(parents=True, exist_ok=True)
library = Path(os.environ["XDG_DATA_HOME"]) / "kiri"
report = {
    "success": False,
    "session": "X11 / Xvfb / Openbox / 1280x800 / scale 1",
    "rendering": "software GL, WebKit compositing disabled for Xvfb only",
    "rendering_environment": {name: os.environ.get(name) for name in (
        "LIBGL_ALWAYS_SOFTWARE", "WEBKIT_DISABLE_COMPOSITING_MODE", "RUST_LOG", "RUST_BACKTRACE"
    )},
    "ui_ready": [],
    "capture_overlay_geometry": [],
    "checks": [],
    "not_tested": ["GNOME Wayland and portal consent", "real audio devices", "multiple monitors", "fractional scaling"],
}
process = None
manager = None
fixture = None
logs = []
recording_region = RECORDING_REGION
recording_size = (680, 380)
temporary = Path(os.environ["TMPDIR"])


def command(*arguments, check=True, binary=False):
    return subprocess.run(arguments, check=check, capture_output=True, text=not binary, timeout=12)


def pump_events():
    # Accessibility state changes arrive on GLib's main context, even though
    # this script has no GTK main loop. The source's Tk loop runs independently.
    context = GLib.MainContext.default()
    for _ in range(50):
        if not context.pending():
            break
        context.iteration(False)


def wait_for(description, predicate, timeout=30):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        pump_events()
        if process is not None and process.poll() is not None:
            raise RuntimeError(f"Kiri exited during {description}: {process.returncode}")
        result = predicate()
        if result:
            return result
        time.sleep(0.1)
    raise RuntimeError(f"Timed out: {description}")


def pause(seconds=0.6):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        pump_events()
        time.sleep(0.05)


def windows():
    result = command("xdotool", "search", "--onlyvisible", "--pid", str(process.pid), check=False)
    return result.stdout.split() if result.returncode == 0 else []


def geometry(window):
    result = command("xdotool", "getwindowgeometry", "--shell", str(window), check=False)
    return dict(line.split("=", 1) for line in result.stdout.splitlines() if "=" in line)


def overlay():
    for window in windows():
        bounds = geometry(window)
        if int(bounds.get("WIDTH", 0)) >= 1278 and int(bounds.get("HEIGHT", 0)) >= 798:
            return window
    return None


def check_overlay_geometry(window, evidence, phase):
    # Query the client window in root coordinates, not its WM frame or a
    # parent-relative position. A decorated first map can otherwise leave a
    # borderless full-size overlay shifted by the removed title bar/border.
    result = command("xwininfo", "-id", str(window))
    actual = {}
    for key, label in (("x", "Absolute upper-left X"), ("y", "Absolute upper-left Y"),
                       ("width", "Width"), ("height", "Height")):
        match = re.search(rf"^\s*{re.escape(label)}:\s*(-?\d+)\s*$", result.stdout, re.MULTILINE)
        if match is None:
            raise RuntimeError(f"Cannot read capture overlay {key} from xwininfo: {result.stdout}")
        actual[key] = int(match.group(1))
    sample = {"phase": phase, "elapsed_seconds": round(time.monotonic() - evidence["started"], 3),
              "actual": actual}
    evidence["samples"].append(sample)
    if actual != evidence["expected"]:
        (output / "failure-overlay-xwininfo.txt").write_text(result.stdout, encoding="utf-8")
        raise RuntimeError(f"Capture overlay geometry at {phase} on attempt {evidence['attempt']}: "
                           f"got {actual}, expected {evidence['expected']}; "
                           "see capture_overlay_geometry and failure-overlay-xwininfo.txt")


def screenshot(name):
    picture = ImageGrab.grab().convert("RGB")
    picture.save(output / name)
    return picture


def assets():
    index = library / "library.json"
    if not index.is_file():
        return []
    return json.loads(index.read_text(encoding="utf-8"))


def start():
    global process
    # The external fixture must not cover the library acceptance screenshot.
    # Keep it withdrawn until the actual app controls have painted.
    fixture.request("hide")
    log = (output / f"app-{len(logs) + 1}.log").open("wb")
    logs.append(log)
    process = subprocess.Popen([str(args.executable.resolve())], stdout=log, stderr=subprocess.STDOUT)
    wait_for("mapped library window", windows)
    wait_for_control("Settings")


def stop(child):
    if child is not None and child.poll() is None:
        child.terminate()
        try:
            child.wait(timeout=10)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait(timeout=5)


def show_fixture():
    # The library can remain above the Tk source even after lift() on Openbox.
    # Minimize only Kiri's isolated QA windows before taking reference pixels.
    for window in windows():
        command("xdotool", "windowminimize", window)
    fixture.request("show")
    wait_for("public fixture is in front", lambda: (
        (image := ImageGrab.grab().convert("RGB")).getpixel((300, 450))[0] < 40
        and 90 < image.getpixel((500, 450))[0] < 150
        and image.getpixel((680, 450))[0] > 190
    ))


def open_capture():
    source = ImageGrab.grab().convert("RGB")
    # linux-native.sh creates one Xvfb monitor at the root origin and scale 1;
    # its real captured desktop dimensions are the required overlay bounds.
    evidence = {"attempt": len(report["capture_overlay_geometry"]) + 1,
                "expected": {"x": 0, "y": 0, "width": source.width, "height": source.height},
                "started": time.monotonic(), "samples": []}
    report["capture_overlay_geometry"].append(evidence)
    command("xdotool", "key", "--clearmodifiers", "ctrl+shift+a")
    window = wait_for("mapped full-display capture overlay", overlay)
    evidence["window"] = window
    check_overlay_geometry(window, evidence, "first observed visible map")
    # A mapped native window says nothing about WebKit readiness. Require a
    # real accessible control plus painted pixels differing from the source.
    wait_for_control("Screenshot", source=source)
    check_overlay_geometry(window, evidence, "painted Screenshot control")
    # Sample before any click/drag so a later configure request cannot conceal
    # an initially offset overlay or introduce a visible geometry jump.
    for index in range(5):
        pause(0.1)
        check_overlay_geometry(window, evidence, f"stable sample {index + 1}")
    return window


def compare(actual, expected, description):
    if actual.size != expected.size:
        raise RuntimeError(f"{description}: got {actual.size}, expected {expected.size}")
    difference = ImageChops.difference(actual.convert("RGB"), expected.convert("RGB"))
    error = sum(ImageStat.Stat(difference).mean) / 3
    if error > 1.5:
        difference.save(output / "pixel-difference.png")
        raise RuntimeError(f"{description}: mean pixel error {error:.3f}")
    return round(error, 4)


def enable_accessibility():
    connection = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    for property_name in ("IsEnabled", "ScreenReaderEnabled"):
        connection.call_sync(
            "org.a11y.Bus", "/org/a11y/bus", "org.freedesktop.DBus.Properties", "Set",
            GLib.Variant("(ssv)", ("org.a11y.Status", property_name, GLib.Variant("b", True))),
            None, Gio.DBusCallFlags.NONE, 5000, None,
        )
    Atspi.init()
    Gst.init(None)


def accessible_nodes():
    desktop = Atspi.get_desktop(0)
    stack = []
    for index in range(desktop.get_child_count()):
        application = desktop.get_child_at_index(index)
        if application and application.get_process_id() == process.pid:
            stack.append(application)
    visited = 0
    while stack and visited < 2000:
        node = stack.pop()
        visited += 1
        try:
            children = [node.get_child_at_index(index) for index in range(node.get_child_count())]
            stack.extend(child for child in reversed(children) if child is not None)
            yield node
        except GLib.Error:
            # A closed WebKit overlay can become defunct during traversal.
            continue


def visible_control(label):
    for node in accessible_nodes():
        try:
            states = node.get_state_set()
            if (node.get_name() != label or not states.contains(Atspi.StateType.SHOWING)
                    or not states.contains(Atspi.StateType.ENABLED)):
                continue
            if "button" not in node.get_role_name() and "switch" not in node.get_role_name():
                continue
            bounds = node.get_component_iface().get_extents(Atspi.CoordType.SCREEN)
            if bounds.width > 0 and bounds.height > 0 and bounds.x >= 0 and bounds.y >= 0:
                return node, bounds
        except GLib.Error:
            continue
    return None


def wait_for_control(label, source=None):
    wait_for(f"accessible UI control: {label}", lambda: visible_control(label))

    def painted():
        control = visible_control(label)
        if control is None:
            return None
        node, bounds = control
        region = (bounds.x, bounds.y, bounds.x + bounds.width, bounds.y + bounds.height)
        desktop = ImageGrab.grab().convert("RGB")
        if region[0] < 0 or region[1] < 0 or region[2] > desktop.width or region[3] > desktop.height:
            raise RuntimeError(f"UI control {label!r} extends beyond the screen: "
                               f"bounds={region}, screen={desktop.size}")
        pixels = desktop.crop(region)
        grayscale = pixels.convert("L")
        low, high = grayscale.getextrema()
        deviation = ImageStat.Stat(grayscale).stddev[0]
        # Text/icon contrast rejects a DOM that exists in a blank WebKit
        # backing surface. This is readiness evidence, not an OCR substitute.
        if high - low < 48 or deviation < 8:
            return None
        source_difference = None
        if source is not None:
            difference = ImageChops.difference(pixels, source.crop(region))
            source_difference = sum(ImageStat.Stat(difference).mean) / 3
            if source_difference < 2:
                return None
        return node, bounds, {
            "control": label, "bounds": list(region), "pixel_range": high - low,
            "pixel_stddev": round(deviation, 3),
            "difference_from_source": round(source_difference, 3) if source_difference is not None else None,
        }

    node, bounds, evidence = wait_for(f"painted UI control: {label}", painted)
    report["ui_ready"].append(evidence)
    return node, bounds


def click_control(label):
    _, bounds = wait_for_control(label)
    command("xdotool", "mousemove", "--sync", str(bounds.x + bounds.width // 2),
            str(bounds.y + bounds.height // 2), "click", "1")
    pause(0.2)


def inline_text_editor():
    for node in accessible_nodes():
        try:
            if node.get_name() != "Text content":
                continue
            if not node.get_state_set().contains(Atspi.StateType.FOCUSED):
                continue
            # GI returns the same Accessible object for get_text_iface(); its
            # get_text() accessor shadows Text.get_text(start, end). Select
            # the interface method explicitly rather than calling the accessor.
            return node, Atspi.Text.get_text(node, 0, -1)
        except (GLib.Error, AttributeError):
            continue
    return None


def drag_region(bounds):
    left, top, right, bottom = bounds
    command("xdotool", "mousemove", str(left), str(top), "mousedown", "1")
    pause(0.15)
    command("xdotool", "mousemove", "--sync", str(right), str(bottom))
    pause(0.15)
    command("xdotool", "mouseup", "1")
    pause(0.3)


def asset_path(asset):
    filename = Path(asset["filename"])
    if filename.name != str(filename):
        raise RuntimeError("Capture filename must remain inside the isolated library")
    return library / "Assets" / filename


def normalized(text):
    return " ".join(text.split())


def wait_for_feedback_to_close():
    # Completion cards are ordinary visible Kiri windows. Wait for their real
    # auto-dismissal before photographing the source, rather than hiding them.
    def closed():
        for window in windows():
            bounds = geometry(window)
            if int(bounds.get("WIDTH", 0)) <= 400 and int(bounds.get("HEIGHT", 0)) <= 180:
                return False
        return True
    wait_for("completion feedback dismissed", closed, timeout=12)


def draw_recording_pattern(stage):
    fixture.request("draw", stage=stage)
    pause(0.15)


def recording_files(pattern):
    return set(temporary.glob(pattern))


def hold_without_controls(seconds):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        visible = []
        for window in windows():
            bounds = geometry(window)
            # GTK can own tiny input-only/selection windows; these are not UI.
            if int(bounds.get("WIDTH", 0)) > 16 and int(bounds.get("HEIGHT", 0)) > 16:
                visible.append(window)
        if visible:
            raise RuntimeError(f"Kiri windows are visible while recording or paused: {visible}")
        pause(0.1)


def discover_video(path):
    info = GstPbutils.Discoverer.new(5 * Gst.SECOND).discover_uri(path.resolve().as_uri())
    if info.get_result() != GstPbutils.DiscovererResult.OK:
        raise RuntimeError(f"GStreamer could not discover the saved MP4: {info.get_result()}")
    streams = info.get_video_streams()
    if len(streams) != 1 or info.get_audio_streams():
        raise RuntimeError("The silent MP4 must contain exactly one video stream and no audio")
    dimensions = (streams[0].get_width(), streams[0].get_height())
    if dimensions != recording_size:
        raise RuntimeError(f"MP4 dimensions {dimensions} do not match region {recording_size}")
    duration = info.get_duration() / Gst.SECOND
    if not 0 < duration < 60:
        raise RuntimeError(f"Invalid MP4 duration: {duration}")
    return duration


def frame_error(actual, expected):
    difference = ImageChops.difference(actual, expected)
    mean = sum(ImageStat.Stat(difference).mean) / 3
    # H.264 4:2:0 subsamples the coloured antialiasing at text edges. Keep the
    # RGB mean check, but detect unexpected UI shapes in luminance so normal
    # chroma loss does not look like an overlay over otherwise correct text.
    luminance = ImageChops.difference(actual.convert("L"), expected.convert("L"))
    histogram = luminance.histogram()
    changed = sum(histogram[33:]) / (actual.width * actual.height)
    return mean, changed


def inspect_recording(path, references, expected_active_seconds, wall_seconds, paused_seconds):
    duration = discover_video(path)
    # Timing allows capture startup/finalization scheduling, but not the
    # deliberately long paused interval. Every decoded frame is checked too.
    if abs(duration - expected_active_seconds) > 1.5:
        raise RuntimeError(f"MP4 duration {duration:.3f}s differs from active time "
                           f"{expected_active_seconds:.3f}s")
    if duration >= wall_seconds - paused_seconds + 1.0:
        raise RuntimeError("MP4 duration includes the paused interval")
    pipeline = Gst.parse_launch(
        "uridecodebin name=decode ! videoconvert ! video/x-raw,format=RGB ! "
        "appsink name=frames sync=false max-buffers=2 drop=false"
    )
    pipeline.get_by_name("decode").set_property("uri", path.resolve().as_uri())
    sink = pipeline.get_by_name("frames")
    bus = pipeline.get_bus()
    counts = {"initial": 0, "resumed": 0}
    largest_mean = 0.0
    largest_changed = 0.0
    previous_pts = None
    final_frame = None
    reached_eos = False
    pipeline.set_state(Gst.State.PLAYING)
    deadline = time.monotonic() + 25
    try:
        while time.monotonic() < deadline:
            sample = sink.emit("try-pull-sample", 200 * Gst.MSECOND)
            if sample is None:
                message = bus.pop_filtered(Gst.MessageType.ERROR)
                if message:
                    error, debug = message.parse_error()
                    raise RuntimeError(f"GStreamer MP4 decode failed: {error}: {debug}")
                if sink.get_property("eos"):
                    reached_eos = True
                    break
                continue
            buffer = sample.get_buffer()
            info = GstVideo.VideoInfo.new_from_caps(sample.get_caps())
            if (info.width, info.height) != recording_size:
                raise RuntimeError("Decoded frame dimensions changed")
            if buffer.pts == Gst.CLOCK_TIME_NONE or (previous_pts is not None and buffer.pts <= previous_pts):
                raise RuntimeError("Decoded MP4 timestamps are missing or not strictly increasing")
            previous_pts = buffer.pts
            mapped, mapping = buffer.map(Gst.MapFlags.READ)
            if not mapped:
                raise RuntimeError("Could not read the decoded MP4 frame")
            try:
                frame = Image.frombytes("RGB", recording_size, bytes(mapping.data)[info.offset[0]:],
                                        "raw", "RGB", info.stride[0], 1)
            finally:
                buffer.unmap(mapping)
            errors = {stage: frame_error(frame, reference) for stage, reference in references.items()}
            closest = min(errors, key=lambda stage: errors[stage][0])
            mean, changed = errors[closest]
            w, h = frame.size
            edges = ((0, 0, w, 2), (0, h - 2, w, h), (0, 0, 2, h), (w - 2, 0, w, h))
            edge_error = max(frame_error(frame.crop(edge), references[closest].crop(edge))[0] for edge in edges)
            # H.264 is lossy; a small mean error and a strict outlier-pixel
            # budget admit text-edge ringing but reject control/toast overlays.
            if closest == "paused" or mean > 3.5 or changed > 0.008 or edge_error > 8:
                frame.save(output / "unexpected-recording-frame.png")
                references[closest].save(output / "unexpected-recording-reference.png")
                ImageChops.difference(frame, references[closest]).save(output / "recording-frame-difference.png")
                (output / "unexpected-recording-frame.json").write_text(json.dumps({
                    "frame_index": sum(counts.values()), "pts_ns": buffer.pts,
                    "nearest_reference": closest, "mean_error": mean,
                    "changed_fraction": changed, "edge_error": edge_error,
                }, indent=2) + "\n", encoding="utf-8")
                raise RuntimeError(f"Unexpected MP4 frame: nearest={closest}, "
                                   f"mean error={mean:.3f}, changed pixels={changed:.4%}, edge error={edge_error:.3f}")
            if sum(counts.values()) == 0:
                frame.save(output / "recording-frame-zero.png")
            if counts[closest] == 0:
                frame.save(output / f"recording-first-{closest}-frame.png")
            counts[closest] += 1
            largest_mean = max(largest_mean, mean)
            largest_changed = max(largest_changed, changed)
            final_frame = frame
        if not reached_eos:
            raise RuntimeError("GStreamer did not decode the complete MP4 before the deadline")
    finally:
        pipeline.set_state(Gst.State.NULL)
    if min(counts.values()) < 20:
        raise RuntimeError(f"Both active recording sections must have at least 20 decoded frames: {counts}")
    final_frame.save(output / "recording-final-frame.png")
    return {
        "dimensions": list(recording_size), "audio_streams": 0,
        "duration_seconds": round(duration, 3),
        "active_seconds": round(expected_active_seconds, 3),
        "paused_seconds": round(paused_seconds, 3),
        "wall_seconds": round(wall_seconds, 3), "decoded_frames": counts,
        "paused_pattern_frames": 0, "unexpected_frames": 0,
        "frame_zero_included": True,
        "selection_edges_checked_on_every_frame": True,
        "maximum_frame_mean_error": round(largest_mean, 4),
        "maximum_frame_changed_fraction": round(largest_changed, 5),
    }


try:
    enable_accessibility()
    manager_log = (output / "openbox.log").open("wb")
    logs.append(manager_log)
    manager = subprocess.Popen(["openbox"], stdout=manager_log, stderr=subprocess.STDOUT)
    wait_for("Openbox window manager", lambda: "window id" in command(
        "xprop", "-root", "_NET_SUPPORTING_WM_CHECK", check=False).stdout)

    # A separate ordinary X11 program owns this public test window. Kiri must
    # capture the screen through its shipping backend, not an injected image.
    fixture = DesktopFixture(output)
    report["source_event_loop"] = "independent Tk process; continuously handles X11 Expose while driver blocks"

    start()
    screenshot("library-launch.png")
    if assets():
        raise RuntimeError("QA must start with an empty isolated library")
    report["checks"].append("installed app paints an accessible Settings control in the visible library of an empty isolated profile")

    show_fixture()
    source_before_probe = screenshot("source-independent-before.png")
    fixture.request("probe_expose")
    # Deliberately do not pump controller events: the ordinary source app must
    # process its own Expose while xdotool/accessibility/CLI calls block us.
    time.sleep(0.35)
    compare(screenshot("source-independent-after.png"), source_before_probe,
            "Source must repaint independently of the driver")
    report["checks"].append("public source repaints an X11 Expose while its driver is blocked")
    for attempt in range(3):
        open_capture()
        screenshot("capture-overlay.png" if attempt == 0 else f"capture-overlay-reopen-{attempt + 1}.png")
        command("xdotool", "key", "--clearmodifiers", "Escape")
        wait_for("Escape closes the overlay", lambda: overlay() is None)
        if assets():
            raise RuntimeError("Cancelled capture unexpectedly saved an asset")
    report["checks"].append("native global shortcut opens capture three times; Escape cancels without saving")
    report["checks"].append("capture overlay matches the root display at first observed visible map, painted readiness, and five stability samples before interaction")

    # Real WebKitGTK key input: verifies browser history and GTK key routing,
    # rather than treating an unprevented DOM key as proof of native Undo.
    show_fixture()
    open_capture()
    drag_region((200, 180, 1000, 700))
    click_control("Rectangle (R)")
    drag_region((420, 460, 620, 560))
    click_control("Text (T)")
    command("xdotool", "mousemove", "--sync", "650", "350", "click", "1")
    wait_for("focused native annotation textarea", inline_text_editor)
    command("xdotool", "type", "--clearmodifiers", "--delay", "40", "alpha beta")
    wait_for("native text has typed prefix", lambda: (
        (editor := inline_text_editor()) and editor[1] == "alpha beta"))
    command("xdotool", "key", "--clearmodifiers", "End")
    command("xdotool", "type", "--clearmodifiers", "x")
    wait_for("native text has appended character", lambda: (
        (editor := inline_text_editor()) and editor[1] == "alpha betax"))
    screenshot("text-before-undo.png")
    command("xdotool", "key", "--clearmodifiers", "ctrl+z")
    undone = wait_for("native text undo changes input without leaving edit", lambda: (
        (editor := inline_text_editor()) and editor[1] != "alpha betax" and editor))
    screenshot("text-after-undo.png")
    command("xdotool", "key", "--clearmodifiers", "ctrl+shift+z")
    wait_for("native text redo restores appended character", lambda: (
        (editor := inline_text_editor()) and editor[1] == "alpha betax"))
    screenshot("text-after-redo.png")
    command("xdotool", "key", "--clearmodifiers", "Escape")
    wait_for("Escape exits only text editing", lambda: inline_text_editor() is None and overlay() is not None)
    undo_control = wait_for_control("Undo (⌘Z)")[0]
    if not undo_control.get_state_set().contains(Atspi.StateType.ENABLED):
        raise RuntimeError("Cancelling text editing lost the earlier rectangle history")
    command("xdotool", "key", "--clearmodifiers", "Escape")
    wait_for("second Escape cancels capture", lambda: overlay() is None)
    if assets():
        raise RuntimeError("Text undo/cancel acceptance unexpectedly saved an image")
    report["text_undo"] = {"before": "alpha betax", "after": undone[1],
                           "redo": "alpha betax", "native_webkitgtk": True}
    report["checks"].append("real focused WebKitGTK textarea Undo/Redo owns text; two-stage Escape preserves prior canvas history")

    show_fixture()
    desktop = screenshot("source-desktop.png")
    expected = desktop.crop((220, 240, 900, 620))
    expected.save(output / "expected-region.png")
    open_capture()
    drag_region((220, 240, 900, 620))
    wait_for_control("Done — Copy to clipboard · Return")
    preview = screenshot("selected-region.png")
    # Exclude handles and the 1px selection border, then verify what the user
    # sees is the same region that will be copied and saved.
    report["preview_mean_pixel_error"] = compare(
        preview.crop((228, 248, 892, 612)), expected.crop((8, 8, 672, 372)),
        "selected-region preview")
    command("xdotool", "key", "--clearmodifiers", "Return")
    saved = wait_for("Return saves one captured image", lambda: assets() if len(assets()) == 1 else None)
    wait_for("successful capture closes its overlay", lambda: overlay() is None)
    asset = saved[0]
    if asset["kind"] != "image" or (asset["pixelWidth"], asset["pixelHeight"]) != (680, 380):
        raise RuntimeError(f"Unexpected capture metadata: {asset}")
    captured = Image.open(asset_path(asset)).convert("RGB")
    captured.save(output / "saved-capture.png")
    report["capture_mean_pixel_error"] = compare(captured, expected, "saved native screenshot")
    report["checks"].append("region drag and Return save the exact pixels from the real X11 desktop")

    clipboard = command("xclip", "-selection", "clipboard", "-t", "image/png", "-o", binary=True).stdout
    clipboard_image = Image.open(io.BytesIO(clipboard)).convert("RGB")
    report["clipboard_mean_pixel_error"] = compare(clipboard_image, captured, "clipboard image")
    clipboard_image.save(output / "clipboard.png")
    report["checks"].append("clipboard contains the captured PNG")

    show_fixture()
    open_capture()
    click_control("OCR")
    drag_region((220, 240, 900, 350))
    wait_for("OCR result Copy button", lambda: visible_control("Copy"), timeout=35)
    screenshot("ocr-recognized-text.png")
    click_control("Copy")
    wait_for("Copy closes the OCR overlay", lambda: overlay() is None)
    recognized = command("xclip", "-selection", "clipboard", "-t", "UTF8_STRING", "-o").stdout
    if "SCREEN CAPTURE 123" not in normalized(recognized):
        raise RuntimeError(f"OCR clipboard text did not contain the public fixture: {recognized!r}")
    (output / "ocr-clipboard.txt").write_text(recognized, encoding="utf-8")
    history = wait_for("recognized text saved to local history", lambda: [
        item for item in assets() if "SCREEN CAPTURE 123" in normalized(item.get("ocrText", ""))
    ])
    if len(history) != 1 or len(assets()) != 2:
        raise RuntimeError("OCR must save exactly one local text-history source image")
    report["ocr_text"] = normalized(recognized)
    report["checks"].append("real OCR mode recognizes public desktop text; Copy writes it to the clipboard and Text History")

    wait_for_feedback_to_close()
    show_fixture()
    draw_recording_pattern("initial")
    references = {"initial": screenshot("recording-source-initial.png").crop(recording_region)}
    open_capture()
    click_control("Record")
    drag_region(recording_region)
    click_control("MP4")
    screenshot("recording-options.png")
    # Exercise the startup race: no countdown must still unmap the selection
    # before the very first captured frame. The decoder below checks all frames.
    countdown, _ = wait_for_control("3-second countdown")
    if countdown.get_state_set().contains(Atspi.StateType.CHECKED):
        click_control("3-second countdown")
    def countdown_is_off():
        control = visible_control("3-second countdown")
        return control and not control[0].get_state_set().contains(Atspi.StateType.CHECKED)
    wait_for("countdown disabled", countdown_is_off)
    report["recording_countdown_enabled"] = False
    staged_before = recording_files(".kiri-media-*.mp4")
    click_control("Start Recording")
    # Pointer stays outside the selected region, including during countdown.
    command("xdotool", "mousemove", "--sync", "1200", "760")
    wait_for("countdown and overlay close before recording", lambda: overlay() is None, timeout=15)
    wait_for("native recorder stages the first MP4", lambda: recording_files(".kiri-media-*.mp4") - staged_before)
    first_started = time.monotonic()
    hold_without_controls(3.0)
    screenshot("recording-running.png")
    pause_requested = time.monotonic()
    command(str(args.executable.resolve()), "--toggle-recording-pause")
    # A finished native segment is atomically published only after the
    # production encoder drains and validates it. CLI return alone is not an
    # acknowledgement that the asynchronous pause has finished.
    first_segment = wait_for("pause finalizes the first native segment", lambda: next(iter(
        recording_files("kiri-recording-*.mp4")), None), timeout=25)
    first_duration = discover_video(first_segment)
    pause(0.2)
    draw_recording_pattern("paused")
    references["paused"] = screenshot("recording-source-paused.png").crop(recording_region)
    paused_at = time.monotonic()
    hold_without_controls(3.0)
    paused_seconds = time.monotonic() - paused_at
    draw_recording_pattern("resumed")
    references["resumed"] = screenshot("recording-source-resumed.png").crop(recording_region)
    staged_before_resume = recording_files(".kiri-media-*.mp4")
    command(str(args.executable.resolve()), "--toggle-recording-pause")
    wait_for("resume starts a second native segment", lambda:
             recording_files(".kiri-media-*.mp4") - staged_before_resume, timeout=25)
    second_started = time.monotonic()
    hold_without_controls(3.0)
    screenshot("recording-resumed.png")
    stop_requested = time.monotonic()
    command(str(args.executable.resolve()), "--stop-recording")
    videos = wait_for("stop imports the completed MP4", lambda:
                      [item for item in assets() if item["kind"] == "video"], timeout=40)
    if len(videos) != 1 or len(assets()) != 3:
        raise RuntimeError("The recording must save exactly one merged video")
    video = videos[0]
    if (video["pixelWidth"], video["pixelHeight"]) != recording_size:
        raise RuntimeError(f"Unexpected recorded region metadata: {video}")
    saved_video = output / "native-recording.mp4"
    shutil.copyfile(asset_path(video), saved_video)
    report["recording"] = inspect_recording(
        saved_video, references, (pause_requested - first_started) + (stop_requested - second_started),
        stop_requested - first_started, paused_seconds,
    )
    report["recording"]["first_segment_seconds"] = round(first_duration, 3)
    if abs(video.get("duration", 0) - report["recording"]["duration_seconds"]) > 0.2:
        raise RuntimeError("Saved video duration metadata does not match the actual MP4")
    report["checks"].append("real MP4 recording starts in the GUI and pauses, resumes, and stops through public CLI controls")
    report["checks"].append("system GStreamer fully decodes the 680x380 silent MP4; active frames match the desktop, with no paused frames or Kiri controls")
    saved = assets()

    stop(process)
    process = None
    start()
    if assets() != saved:
        raise RuntimeError("Capture, OCR, or video library changed after restarting the installed app")
    screenshot("library-reopened.png")
    report["checks"].append("saved screenshot, OCR history, and recording persist after restarting the installed app")
    report["success"] = True
except Exception as error:
    report["error"] = str(error)
    try:
        screenshot("failure-desktop.png")
        report["windows"] = {window: geometry(window) for window in windows()} if process else {}
        report["window_manager"] = command(
            "xprop", "-root", "_NET_ACTIVE_WINDOW", "_NET_CLIENT_LIST_STACKING", check=False,
        ).stdout
        report["pointer"] = command("xdotool", "getmouselocation", "--shell", check=False).stdout
        (output / "failure-x11-tree.txt").write_text(
            command("xwininfo", "-root", "-tree", check=False).stdout, encoding="utf-8",
        )
        if process is not None and process.poll() is None:
            report["accessibility"] = [
                {"name": node.get_name(), "role": node.get_role_name()}
                for node in accessible_nodes()
            ]
    except Exception:
        pass
finally:
    stop(process)
    if fixture is not None:
        fixture.close()
    stop(manager)
    for log in logs:
        log.close()
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")

if not report["success"]:
    raise SystemExit(report["error"])
print(json.dumps(report, indent=2))
