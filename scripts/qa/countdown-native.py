"""Native acceptance on an isolated Windows CI desktop; capture protection stays on."""
import ctypes,json,os,re,subprocess,time
from pathlib import Path
from pywinauto import Desktop,keyboard,mouse

if os.environ.get('GITHUB_ACTIONS')!='true':raise SystemExit('Use an isolated CI desktop')
OUT=Path('countdown-native-review');OUT.mkdir(exist_ok=True)
report={'success':False,'native':True,'checks':[]}
app=Path('src-tauri/target/release/kiri.exe').resolve()
proc=subprocess.Popen([str(app)]);pid=proc.pid
desktop=Desktop(backend='uia');source_window=None
width=ctypes.windll.user32.GetSystemMetrics(0);height=ctypes.windll.user32.GetSystemMetrics(1)
def windows():return desktop.windows(process=pid,visible_only=True)
def find(name,kind='Button',timeout=15):
 end=time.monotonic()+timeout
 while time.monotonic()<end:
  if proc.poll() is not None:raise RuntimeError('Kiri exited: '+str(proc.returncode))
  for w in windows():
   for c in w.descendants(control_type=kind):
    try:
     if re.fullmatch(name,c.window_text()) and c.is_visible() and c.is_enabled():return c
    except Exception:pass
  time.sleep(.1)
 raise RuntimeError('Visible control not found: '+name)
def exists(name):
 try:find(name,timeout=.5);return True
 except RuntimeError:return False
def begin():
 source_window.set_focus();time.sleep(.3)
 keyboard.send_keys('^+a');find('Record').click_input()
 x1,y1=int(width*.2),int(height*.2);x2,y2=int(width*.8),int(height*.65)
 mouse.press(coords=(x1,y1))
 for step in range(1,21):
  ctypes.windll.user32.SetCursorPos(round(x1+(x2-x1)*step/20),round(y1+(y2-y1)*step/20));time.sleep(.035)
 mouse.release(coords=(x2,y2));find('Start Recording').click_input()
 return find('Cancel Countdown',timeout=12)
def cancelled():
 time.sleep(3.5)
 if proc.poll() is not None:raise RuntimeError('Cancellation exited the application')
 if exists('Pause Recording') or exists('Cancel Countdown'):raise RuntimeError('Cancelled flow remains active')
try:
 find('Settings',timeout=35)
 sample=Path(os.environ['RUNNER_TEMP'])/'kiri-countdown-check.txt'
 sample.write_text('An original local recording sample.\nCancel, then start a new recording.\n',encoding='utf-8')
 subprocess.Popen(['notepad.exe',str(sample)])
 source_window=desktop.window(title_re='.*kiri-countdown-check.*')
 source_window.wait('visible',timeout=20);source_window.set_focus();time.sleep(.5)
 for mode in ['click','Escape']:
  cancel=begin();parent=cancel.top_level_parent();h=parent.handle
  if not parent.is_visible():raise RuntimeError('Countdown is not visible')
  if ctypes.windll.user32.GetForegroundWindow()!=h:raise RuntimeError('Countdown lacks native focus')
  affinity=ctypes.c_uint()
  ok=ctypes.windll.user32.GetWindowDisplayAffinity(h,ctypes.byref(affinity))
  if not ok or affinity.value!=0x11:raise RuntimeError('Countdown capture exclusion lost')
  if mode=='click':cancel.click_input()
  else:keyboard.send_keys('{ESC}')
  cancelled();report['checks'].append(mode+' cancellation; visible, focused and capture-protected')
 cancel=begin();seen=set();deadline=time.monotonic()+4
 while time.monotonic()<deadline:
  try:
   for t in cancel.top_level_parent().descendants(control_type='Text'):
    match=re.fullmatch(r'(?:Recording starts in )?([123])',t.window_text())
    if match:seen.add(match.group(1))
  except Exception:pass
  time.sleep(.12)
 report['observed_digits']=sorted(seen)
 if not {'1','2'}.issubset(seen):raise RuntimeError('Countdown digits did not advance visibly')
 pause=find('Pause Recording',timeout=35);stop=find('Stop and Save Recording');panel=stop.top_level_parent()
 affinity=ctypes.c_uint();ctypes.windll.user32.GetWindowDisplayAffinity(panel.handle,ctypes.byref(affinity))
 if affinity.value!=0x11:raise RuntimeError('Recording controls lost capture exclusion')
 time.sleep(1);pause.click_input();find('Resume Recording').click_input();time.sleep(1)
 find('Stop and Save Recording').click_input();time.sleep(5)
 report['checks'].append('countdown completed; recording started, paused, resumed and stopped')
 report['success']=True
except Exception as error:
 report['error']=str(error)[:1500];report['process_exit_code']=proc.poll();report['windows']=[]
 for w in windows():
  try:report['windows'].append({'title':w.window_text(),'controls':[{'type':c.element_info.control_type,'text':c.window_text()} for c in w.descendants()]})
  except Exception:pass
finally:
 log=Path(os.environ['LOCALAPPDATA'])/'io.yuxino.kiri/logs/kiri.log'
 if log.exists():(OUT/'application.log').write_text('\n'.join(log.read_text(encoding='utf-8',errors='replace').splitlines()[-350:])+'\n',encoding='utf-8')
 (OUT/'report.json').write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf-8')
 subprocess.run(['taskkill','/PID',str(pid),'/T','/F'],capture_output=True)
 if source_window:
  try:source_window.close()
  except Exception:pass
if not report['success']:raise SystemExit(report['error'])
