"""Exercise native shortcut ownership and recording on a disposable CI desktop."""
import ctypes
import json
import os
from pathlib import Path
import re
import subprocess
import time

from pywinauto import Desktop, keyboard

if os.environ.get("GITHUB_ACTIONS") != "true" or os.name != "nt":
    raise SystemExit("Use an isolated Windows CI desktop")

output = Path("shortcut-native-review")
output.mkdir(exist_ok=True)
report = {"success": False, "checks": []}
desktop = Desktop(backend="uia")
executable = Path("src-tauri/target/release/kiri.exe").resolve()
process = None
hotkey_id = 2122
held = False


def find(name, timeout=15):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError("Kiri exited: " + str(process.returncode))
        for window in desktop.windows(process=process.pid, visible_only=True):
            for control in window.descendants():
                try:
                    if re.fullmatch(name, control.window_text()) and control.is_visible() and control.is_enabled():
                        return control
                except Exception:
                    pass
        time.sleep(.1)
    raise RuntimeError("Visible control not found: " + name)


def start():
    global process
    process = subprocess.Popen([str(executable)])
    find("Settings", timeout=35).click_input()
    find("Change Shortcut")


def stop():
    if process and process.poll() is None:
        subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"], capture_output=True)
        process.wait(timeout=15)


try:
    # Own the default before Kiri starts. Kiri must still open its editor.
    held = bool(ctypes.windll.user32.RegisterHotKey(None, hotkey_id, 0x0002 | 0x0004, ord("A")))
    if not held:
        raise RuntimeError("Could not reserve the test hotkey")
    start()
    find("Used by Another App")
    find("Change Shortcut").click_input()
    find(r"Press a new shortcut \(Esc to cancel\)")
    keyboard.send_keys("^%k")
    find(r"Ctrl\+Alt\+K")
    find("Enabled")
    report["checks"].append("occupied startup shortcut can be changed")

    find("Restore Default Shortcut").click_input()
    find(re.escape("This shortcut is unavailable. Choose another combination."))
    find(r"Ctrl\+Alt\+K")
    report["checks"].append("failed replacement preserves the working combination")

    stop()
    start()
    find(r"Ctrl\+Alt\+K")
    find("Enabled")
    report["checks"].append("choice survives process restart")

    # The global callback must confirm an unchanged key without opening capture.
    find("Change Shortcut").click_input()
    find(r"Press a new shortcut \(Esc to cancel\)")
    keyboard.send_keys("^%k")
    find("Change Shortcut")
    find(r"Ctrl\+Alt\+K")
    report["checks"].append("recording the existing key does not trigger capture")

    keyboard.send_keys("^%k")
    find("Screenshot")
    find("Record")
    find("OCR")
    keyboard.send_keys("{ESC}")
    find("Change Shortcut")
    report["checks"].append("custom native hotkey opens and cancels capture")

    ctypes.windll.user32.UnregisterHotKey(None, hotkey_id)
    held = False
    find("Restore Default Shortcut").click_input()
    find(r"Shift\+Ctrl\+A")
    keyboard.send_keys("^+a")
    find("Screenshot")
    keyboard.send_keys("{ESC}")
    find("Change Shortcut")
    report["checks"].append("restored default triggers capture after conflict is released")
    report["success"] = True
except Exception as error:
    report["error"] = str(error)
    report["windows"] = []
    if process and process.poll() is None:
        for window in desktop.windows(process=process.pid, visible_only=True):
            report["windows"].append([control.window_text() for control in window.descendants()])
finally:
    if held:
        ctypes.windll.user32.UnregisterHotKey(None, hotkey_id)
    stop()
    (output / "report.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
if not report["success"]:
    raise SystemExit(report["error"])
