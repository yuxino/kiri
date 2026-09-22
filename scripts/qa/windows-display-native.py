"""Real installed Kiri against an OS virtual display; never substitutes capture data."""
import ctypes
from ctypes import wintypes as w
import hashlib
import json
import os
from pathlib import Path
import subprocess
import struct
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
shcore = ctypes.WinDLL('shcore')
shcore.GetDpiForMonitor.argtypes = [w.HANDLE, ctypes.c_int, ctypes.POINTER(w.UINT), ctypes.POINTER(w.UINT)]


def monitors():
    found = []
    @ctypes.WINFUNCTYPE(w.BOOL, w.HANDLE, w.HDC, ctypes.POINTER(w.RECT), w.LPARAM)
    def callback(handle, dc, rect, data):
        info = MonitorInfo(size=ctypes.sizeof(MonitorInfo))
        if not u.GetMonitorInfoW(handle, ctypes.byref(info)):
            raise ctypes.WinError(ctypes.get_last_error())
        r = info.monitor
        dpi_x, dpi_y = w.UINT(), w.UINT()
        if shcore.GetDpiForMonitor(handle, 0, ctypes.byref(dpi_x), ctypes.byref(dpi_y)) != 0:
            raise RuntimeError('GetDpiForMonitor failed')
        found.append({'device': info.device, 'rect': [r.left, r.top, r.right, r.bottom], 'primary': bool(info.flags & 1), 'dpi': dpi_x.value})
        return True
    if not u.EnumDisplayMonitors(None, None, callback, 0):
        raise ctypes.WinError(ctypes.get_last_error())
    return found


def move_display(device, x, y, primary=False, resize=True, deferred=False):
    mode = DevMode(size=ctypes.sizeof(DevMode))
    if not u.EnumDisplaySettingsW(device, 0xffffffff, ctypes.byref(mode)):
        raise ctypes.WinError(ctypes.get_last_error())
    mode.x, mode.y = x, y
    mode.fields = 0x20  # position
    if resize:
        mode.width, mode.height, mode.frequency = 2560, 1440, 60
        mode.fields |= 0x80000 | 0x100000 | 0x400000  # width, height, frequency
    flags = (0x10 if primary else 0) | (0x10000001 if deferred else 0)
    result = u.ChangeDisplaySettingsExW(device, ctypes.byref(mode), None, flags, None)
    if result != 0:
        raise RuntimeError(f'ChangeDisplaySettingsEx {device} ({x}, {y}), primary={primary}, deferred={deferred}: {result}')
    if not deferred:
        time.sleep(2)


def set_scale(device, percent):
    # Isolated QA only: the Windows DPI request packets are undocumented.
    # Layouts: https://github.com/lihas/windows-DPI-scaling-sample
    paths_count, modes_count = w.UINT(), w.UINT()
    result = u.GetDisplayConfigBufferSizes(2, ctypes.byref(paths_count), ctypes.byref(modes_count))
    if result:
        raise RuntimeError('GetDisplayConfigBufferSizes: ' + str(result))
    paths = ctypes.create_string_buffer(paths_count.value * 72)
    modes = ctypes.create_string_buffer(modes_count.value * 64)
    result = u.QueryDisplayConfig(2, ctypes.byref(paths_count), paths, ctypes.byref(modes_count), modes, None)
    if result:
        raise RuntimeError('QueryDisplayConfig: ' + str(result))
    for i in range(paths_count.value):
        identity = paths.raw[i * 72:i * 72 + 12]
        name = ctypes.create_string_buffer(struct.pack('<II', 1, 84) + identity + bytes(64), 84)
        if u.DisplayConfigGetDeviceInfo(name) != 0:
            continue
        if name.raw[20:].decode('utf-16-le').split('\0')[0] != device:
            continue
        request = ctypes.create_string_buffer(struct.pack('<iI', -3, 32) + identity + bytes(12), 32)
        result = u.DisplayConfigGetDeviceInfo(request)
        if result:
            raise RuntimeError('Read display DPI: ' + str(result))
        minimum, current, maximum = struct.unpack('<iii', request.raw[20:])
        relative = [100, 125, 150, 175, 200, 225, 250, 300, 350, 400, 450, 500].index(percent) + minimum
        if not minimum <= relative <= maximum:
            raise RuntimeError('Requested DPI is outside monitor limits')
        request = ctypes.create_string_buffer(struct.pack('<iI', -4, 24) + identity + struct.pack('<i', relative), 24)
        result = u.DisplayConfigSetDeviceInfo(request)
        if result:
            raise RuntimeError('Set display DPI: ' + str(result))
        time.sleep(2)
        actual = next(m['dpi'] for m in monitors() if m['device'] == device)
        if actual != round(96 * percent / 100):
            raise RuntimeError(f'Display DPI unchanged: {actual}, requested {percent}%')
        return
    raise RuntimeError('Display source not found for scaling')


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
from pywinauto import Desktop, keyboard

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
    virtual = [m for m in report['environment'] if not m['primary']]
    if len(virtual) < 2:
        raise RuntimeError('Two OS-level virtual displays are required for mixed-DPI acceptance')
    # The hosted machine's 1024x768 adapter cannot support high DPI. Use two
    # 1440p OS displays, retaining the original adapter as an additional screen.
    primary, secondary = virtual[:2]
    move_display(primary['device'], 0, 0, primary=True, deferred=True)
    move_display(secondary['device'], 2560, 0, deferred=True)
    original = next(m for m in report['environment'] if m['primary'])
    move_display(original['device'], 0, 1440, resize=False, deferred=True)
    result = u.ChangeDisplaySettingsExW(None, None, None, 0, None)
    if result:
        raise RuntimeError('Apply complete display layout: ' + str(result))
    time.sleep(2)
    primary = next(m for m in monitors() if m['device'] == primary['device'])
    if not primary['primary'] or primary['rect'] != [0, 0, 2560, 1440]:
        raise RuntimeError('The high-resolution test display did not become the actual primary')
    report['test_primary'] = primary
    app = subprocess.Popen([str(exe)])
    wait_for(lambda: Desktop(backend='uia').windows(process=app.pid, visible_only=True))
    time.sleep(2)
    layouts = [(f'{name}-primary{primary_scale}-secondary{scale}', x, y, primary_scale, scale)
               for primary_scale, scale in [(100, 100), (100, 125), (100, 150), (100, 200), (150, 100)]
               for name, x, y in [('right', primary['rect'][2], 0), ('left', -2560, 0), ('above', 0, -1440)]]
    for layout, x, y, primary_scale, scale in layouts:
        move_display(secondary['device'], x, y)
        set_scale(primary['device'], primary_scale)
        set_scale(secondary['device'], scale)
        fixture = subprocess.Popen([sys.executable, __file__, '--fixture', str(x), str(y), '2560', '1440'])
        fixture_windows = wait_for(lambda: Desktop(backend='win32').windows(process=fixture.pid, visible_only=True))
        fixture_window = fixture_windows[0]
        fixture_window.move_window(x, y, 2560, 1440, repaint=True)
        fixture_window.set_focus()
        time.sleep(1)
        source = ImageGrab.grab(bbox=(x + 120, y + 180, x + 680, y + 380), all_screens=True).convert('RGB')
        source.save(out / f'{layout}-source.png')
        if source.getpixel((5, 5)) != (250, 250, 250):
            raise RuntimeError('Native fixture is not visible at its expected secondary display position')
        u.SetCursorPos(x + 900, y + 600)
        keyboard.send_keys('^+a')
        window = wait_for(overlay)
        rect = window.rectangle()
        actual = [rect.left, rect.top, rect.right, rect.bottom]
        expected = [x, y, x + 2560, y + 1440]
        report['checks'].append({'layout': layout, 'monitors': monitors(), 'overlay': actual, 'expected': expected})
        ImageGrab.grab(bbox=tuple(expected), all_screens=True).save(out / f'{layout}-overlay.png')
        if actual != expected:
            raise RuntimeError(f'{layout}: overlay {actual} != monitor {expected}')
        u.SetCursorPos(x + 120, y + 180)
        u.mouse_event(0x0002, 0, 0, 0, 0)
        for step in range(1, 21):
            u.SetCursorPos(x + 120 + 28 * step, y + 180 + 10 * step)
            time.sleep(.02)
        u.mouse_event(0x0004, 0, 0, 0, 0)
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
        print(f'{layout}: native overlay bounds and clipboard pixels verified (mean error {error})', flush=True)
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
