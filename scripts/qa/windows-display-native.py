"""Real installed Kiri against an OS virtual display; never substitutes capture data."""
import ctypes
from ctypes import wintypes as w
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

if os.name != 'nt' or os.environ.get('GITHUB_ACTIONS') != 'true':
    raise SystemExit('Use an isolated Windows CI desktop')
u = ctypes.WinDLL('user32', use_last_error=True)
u.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))

class MonitorInfo(ctypes.Structure):
    _fields_ = [('size', w.DWORD), ('monitor', w.RECT), ('work', w.RECT),
                ('flags', w.DWORD), ('device', w.WCHAR * 32)]

class DevMode(ctypes.Structure):
    _fields_ = [('device', w.WCHAR * 32), ('spec', w.WORD), ('version', w.WORD),
                ('size', w.WORD), ('extra', w.WORD), ('fields', w.DWORD),
                ('x', w.LONG), ('y', w.LONG), ('orientation', w.DWORD), ('fixed', w.DWORD),
                ('color', w.SHORT), ('duplex', w.SHORT), ('yresolution', w.SHORT),
                ('tt', w.SHORT), ('collate', w.SHORT), ('form', w.WCHAR * 32),
                ('logpixels', w.WORD), ('bits', w.DWORD), ('width', w.DWORD),
                ('height', w.DWORD), ('flags', w.DWORD), ('frequency', w.DWORD),
                ('icm', w.DWORD), ('intent', w.DWORD), ('media', w.DWORD),
                ('dither', w.DWORD), ('reserved1', w.DWORD), ('reserved2', w.DWORD),
                ('panwidth', w.DWORD), ('panheight', w.DWORD)]

u.GetMonitorInfoW.argtypes = [w.HANDLE, ctypes.POINTER(MonitorInfo)]
u.EnumDisplaySettingsW.argtypes = [w.LPCWSTR, w.DWORD, ctypes.POINTER(DevMode)]
u.ChangeDisplaySettingsExW.argtypes = [w.LPCWSTR, ctypes.POINTER(DevMode), w.HWND, w.DWORD, ctypes.c_void_p]
u.SetWindowPos.argtypes = [w.HWND, w.HWND, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int, w.UINT]
u.GetParent.argtypes = [w.HWND]
u.GetParent.restype = w.HWND


def monitors():
    found = []
    @ctypes.WINFUNCTYPE(w.BOOL, w.HANDLE, w.HDC, ctypes.POINTER(w.RECT), w.LPARAM)
    def callback(handle, dc, rect, data):
        info = MonitorInfo(size=ctypes.sizeof(MonitorInfo))
        if not u.GetMonitorInfoW(handle, ctypes.byref(info)):
            raise ctypes.WinError(ctypes.get_last_error())
        r = info.monitor
        found.append({'device': info.device, 'rect': [r.left, r.top, r.right, r.bottom], 'primary': bool(info.flags & 1)})
        return True
    if not u.EnumDisplayMonitors(None, None, callback, 0):
        raise ctypes.WinError(ctypes.get_last_error())
    return found


def move_display(device, x, y):
    mode = DevMode(size=ctypes.sizeof(DevMode))
    if not u.EnumDisplaySettingsW(device, 0xffffffff, ctypes.byref(mode)):
        raise ctypes.WinError(ctypes.get_last_error())
    mode.x, mode.y = x, y
    mode.width, mode.height, mode.frequency = 1920, 1080, 60
    mode.fields = 0x20 | 0x80000 | 0x100000 | 0x400000  # position, width, height, frequency
    result = u.ChangeDisplaySettingsExW(device, ctypes.byref(mode), None, 0, None)
    if result != 0:
        raise RuntimeError('ChangeDisplaySettingsEx: ' + str(result))
    time.sleep(2)


if '--fixture' in sys.argv:
    import tkinter as tk
    x, y, width, height = map(int, sys.argv[-4:])
    root = tk.Tk()
    root.title('Kiri native secondary display fixture')
    root.overrideredirect(True)
    root.geometry(f'{width}x{height}+0+0')
    canvas = tk.Canvas(root, bg='#ededed', highlightthickness=0)
    canvas.pack(fill='both', expand=True)
    canvas.create_rectangle(120, 180, 680, 380, fill='#fafafa', outline='')
    canvas.create_text(150, 210, anchor='nw', text='SECONDARY DISPLAY\nKiri native screenshot verification\nLocal pixels stay on this display', font=('Arial', 20), fill='#202020')
    root.update()
    hwnd = u.GetParent(root.winfo_id())
    if not u.SetWindowPos(hwnd, None, x, y, width, height, 0x40):
        raise ctypes.WinError(ctypes.get_last_error())
    root.after(120000, root.destroy)
    root.mainloop()
    raise SystemExit(0)

from PIL import ImageGrab, ImageChops, ImageStat
from pywinauto import Desktop, keyboard, mouse

out = Path('windows-display-review')
out.mkdir(exist_ok=True)
report = {'success': False, 'environment': monitors(), 'checks': []}
app = fixture = None
exe = Path(os.environ['KIRI_QA_EXE'])
report['executable_sha256'] = hashlib.sha256(exe.read_bytes()).hexdigest()


def wait_for(predicate, timeout=25):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        if app and app.poll() is not None:
            raise RuntimeError('Kiri exited: ' + str(app.returncode))
        result = predicate()
        if result:
            return result
        time.sleep(.15)
    raise RuntimeError('Timed out waiting for native Kiri UI')


def overlay():
    for window in Desktop(backend='uia').windows(process=app.pid, visible_only=True):
        try:
            names = {c.window_text() for c in window.descendants()}
            if {'Screenshot', 'Record', 'OCR'} <= names:
                return window
        except Exception:
            pass
    return None


try:
    primary = next(m for m in report['environment'] if m['primary'])
    secondary = next((m for m in report['environment'] if not m['primary']), None)
    if not secondary:
        raise RuntimeError('No OS-level secondary display; acceptance cannot run')
    app = subprocess.Popen([str(exe)])
    wait_for(lambda: Desktop(backend='uia').windows(process=app.pid, visible_only=True))
    time.sleep(2)
    for layout, x, y in [('right', primary['rect'][2], 0), ('left', -1920, 0), ('above', 0, -1080)]:
        move_display(secondary['device'], x, y)
        fixture = subprocess.Popen([sys.executable, __file__, '--fixture', str(x), str(y), '1920', '1080'])
        time.sleep(2)
        source = ImageGrab.grab(bbox=(x + 120, y + 180, x + 680, y + 380), all_screens=True).convert('RGB')
        source.save(out / f'{layout}-source.png')
        u.SetCursorPos(x + 900, y + 600)
        keyboard.send_keys('^+a')
        window = wait_for(overlay)
        rect = window.rectangle()
        actual = [rect.left, rect.top, rect.right, rect.bottom]
        expected = [x, y, x + 1920, y + 1080]
        report['checks'].append({'layout': layout, 'monitors': monitors(), 'overlay': actual, 'expected': expected})
        ImageGrab.grab(bbox=tuple(expected), all_screens=True).save(out / f'{layout}-overlay.png')
        if actual != expected:
            raise RuntimeError(f'{layout}: overlay {actual} != monitor {expected}')
        mouse.press(coords=(x + 120, y + 180))
        mouse.move(coords=(x + 680, y + 380), duration=.5)
        mouse.release(coords=(x + 680, y + 380))
        time.sleep(.5)
        ImageGrab.grab(bbox=tuple(expected), all_screens=True).save(out / f'{layout}-selected.png')
        keyboard.send_keys('{ENTER}')
        time.sleep(2)
        copied = ImageGrab.grabclipboard()
        if not hasattr(copied, 'size'):
            raise RuntimeError('Screenshot did not reach the clipboard')
        copied = copied.convert('RGB')
        copied.save(out / f'{layout}-clipboard.png')
        if copied.size != source.size:
            raise RuntimeError(f'{layout}: clipboard size {copied.size} != {source.size}')
        error = sum(ImageStat.Stat(ImageChops.difference(source, copied)).mean) / 3
        report['checks'][-1]['pixel_mean_error'] = error
        if error > 2:
            raise RuntimeError(f'{layout}: clipboard differs from actual secondary pixels: {error}')
        fixture.terminate()
        fixture.wait(timeout=10)
        fixture = None
    report['success'] = True
except Exception as error:
    report['error'] = str(error)
    ImageGrab.grab(all_screens=True).save(out / 'failure-desktop.png')
finally:
    for process in [fixture, app]:
        if process and process.poll() is None:
            subprocess.run(['taskkill', '/PID', str(process.pid), '/T', '/F'], capture_output=True)
    log = Path(os.environ['LOCALAPPDATA']) / 'io.yuxino.kiri/logs/kiri.log'
    if log.exists():
        (out / 'kiri.log').write_bytes(log.read_bytes())
    (out / 'report.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
if not report['success']:
    raise SystemExit(report['error'])
