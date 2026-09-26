"""Smoke-test the extracted ZIP and installed NSIS app on disposable Windows CI."""

import json
import os
from pathlib import Path
import re
import subprocess
import time

from pywinauto import Desktop, keyboard


if os.name != "nt" or os.environ.get("GITHUB_ACTIONS") != "true":
    raise SystemExit("Use an isolated Windows CI desktop")

output = Path("windows-release-review")
output.mkdir(exist_ok=True)
report = {"success": False, "checks": []}
desktop = Desktop(backend="uia")
process = None


def find(name, timeout=35):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError(f"Kiri exited early: {process.returncode}")
        for window in desktop.windows(process=process.pid, visible_only=True):
            for control in window.descendants():
                try:
                    if (re.fullmatch(name, control.window_text()) and control.is_visible()
                            and control.is_enabled()):
                        return control
                except Exception:
                    pass
        time.sleep(0.1)
    raise RuntimeError(f"Visible control not found: {name}")


def stop():
    global process
    if process and process.poll() is None:
        subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"], capture_output=True)
        process.wait(timeout=15)
    process = None


def smoke(executable, update_button, label):
    global process
    if not executable.is_file():
        raise RuntimeError(f"Missing {label} executable")
    process = subprocess.Popen([str(executable)])
    try:
        find("Settings").click_input()
        find(update_button)
        report["checks"].append(f"{label} launches and shows the correct update route")
        keyboard.send_keys("^+a")
        find("Screenshot")
        keyboard.send_keys("{ESC}")
        report["checks"].append(f"{label} opens and cancels native capture")
    finally:
        stop()


try:
    portable = Path(os.environ["KIRI_QA_PORTABLE_EXE"])
    installed = Path(os.environ["KIRI_QA_INSTALLED_EXE"])
    if not (portable.parent / "kiri.portable").is_file():
        raise RuntimeError("Extracted portable marker is missing")
    if (installed.parent / "kiri.portable").exists():
        raise RuntimeError("Installed copy contains a portable marker")
    smoke(portable, "Open Releases Page", "portable ZIP")
    if sorted(path.name for path in portable.parent.iterdir()) != ["kiri.exe", "kiri.portable"]:
        raise RuntimeError("Portable copy wrote unexpected files beside the executable")
    smoke(installed, "Check for Updates", "NSIS installation")
    report["success"] = True
except Exception as error:
    report["error"] = str(error)[:1500]
    report["windows"] = []
    if process and process.poll() is None:
        for window in desktop.windows(process=process.pid, visible_only=True):
            try:
                report["windows"].append([control.window_text() for control in window.descendants()])
            except Exception:
                pass
finally:
    stop()
    (output / "report.json").write_text(json.dumps(report, indent=2), encoding="utf-8")

if not report["success"]:
    raise SystemExit(report["error"])
