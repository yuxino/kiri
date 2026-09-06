"""One continuous actual-frontend session. Native boundaries are isolated documentation fixtures."""
import argparse,asyncio,hashlib,io,json,os,re,subprocess,threading,time
from http.server import SimpleHTTPRequestHandler,ThreadingHTTPServer
from pathlib import Path
from PIL import Image
from playwright.async_api import async_playwright
ROOT=Path(__file__).resolve().parent
BUILD=Path(os.environ['KIRI_DEMO_DIST']).resolve()
OUT=Path(os.environ['KIRI_DEMO_OUT']).resolve();OUT.mkdir(parents=True,exist_ok=True)
MEDIA={}
class Handler(SimpleHTTPRequestHandler):
 def log_message(self,*args):pass
 def translate_path(self,path):
  path=path.split('?')[0]
  if path.startswith('/app/'):return str(BUILD/path.removeprefix('/app/'))
  if path.startswith('/assets/'):return str(BUILD/path.removeprefix('/'))
  return str(ROOT/path.removeprefix('/'))
 def reply(self,data,ctype):
  self.send_response(200);self.send_header('Content-Type',ctype);self.send_header('Content-Length',str(len(data)));self.send_header('Cache-Control','no-store');self.end_headers();self.wfile.write(data)
 def do_GET(self):
  name=self.path.split('?')[0]
  if name in ['/app/','/app/index.html']:
   # Install only the documentation IPC boundary before the untouched app module.
   html=(BUILD/'index.html').read_text().replace('<head>','<head><script src="/bridge.js"></script>',1)
   self.reply(html.encode(),'text/html; charset=utf-8');return
  if name.startswith('/__media__/'):
   key=name.removeprefix('/__media__/')
   if key.startswith('capture/frozen/'):key='frozen'
   elif key.startswith('thumbnail/'):key='thumb-'+key.split('/')[-1]
   elif key.startswith(('media/','asset/','annotation-source/')):key=key.split('/')[-1]
   result=MEDIA.get(key)
   if not result:self.send_error(404);return
   self.reply(*result);return
  super().do_GET()
async def run(args):
 server=ThreadingHTTPServer(('127.0.0.1',8791),Handler);threading.Thread(target=server.serve_forever,daemon=True).start()
 report={'success':False,'source_tree':os.environ['KIRI_SOURCE_TREE'],'source_commit':os.environ['KIRI_SOURCE_COMMIT'],'artifact_id':None,'artifact_run':int(os.environ['GITHUB_RUN_ID']),'native':False,'single_take':True,'scene_cuts':0,'speed_factor':10,'scenes':[],'checks':{},'errors':[],'http_errors':[]}
 async with async_playwright() as pw:
  browser=await pw.chromium.launch(executable_path=os.environ.get('CHROME_BIN','/usr/bin/google-chrome'))
  context=await browser.new_context(viewport={'width':1280,'height':720},device_scale_factor=1.5,locale='zh-CN',color_scheme='light')
  await context.route('**/*',lambda r:r.continue_() if r.request.url.startswith(('http://127.0.0.1:8791/','blob:http://127.0.0.1:8791/','data:image/')) else r.abort())
  page=await context.new_page();page.set_default_timeout(12000)
  page.on('pageerror',lambda e:report['errors'].append(e.stack or str(e)))
  page.on('response',lambda r:report['http_errors'].append({'url':r.url,'status':r.status}) if r.status>=400 else None)
  subject_task=None;subject_frames=[];capturing=False;paused=False;clip=None;lock=asyncio.Lock()
  async def pixels(**options):
   async with lock:return await page.screenshot(animations='allow',**options)
  async def record_subject():
   while capturing:
    if not paused:
     try:subject_frames.append((time.monotonic(),await pixels(type='png',clip=clip)))
     except Exception:break
    await asyncio.sleep(.3)
  async def backend(source,cmd,a):
   nonlocal capturing,paused,clip,subject_task,subject_frames
   if cmd=='freeze':
    async with lock:
     await page.locator('#pointer').evaluate("e=>e.style.visibility='hidden'")
     data=await page.screenshot(type='png')
     await page.locator('#pointer').evaluate("e=>e.style.visibility='visible'")
    MEDIA['frozen']=(data,'image/png');return None
   if cmd=='save_png':
    data=bytes(a['bytes']);Image.open(io.BytesIO(data)).verify();MEDIA[a['id']]=(data,'image/png');MEDIA['thumb-'+a['id']]=(data,'image/png');return None
   if cmd=='record_start':
    clip=a['region'];capturing=True;paused=False;subject_frames=[];subject_task=asyncio.create_task(record_subject());return None
   if cmd=='record_pause':paused=True;return None
   if cmd=='record_resume':paused=False;return None
   if cmd=='record_stop':
    capturing=False
    if subject_task:await subject_task
    if len(subject_frames)<4:raise RuntimeError('Insufficient actual subject frames')
    folder=OUT/'subject';folder.mkdir(exist_ok=True)
    for i,(t,data) in enumerate(subject_frames):(folder/f'{i:05}.png').write_bytes(data)
    path=folder/'recording.mp4'
    result=await asyncio.to_thread(subprocess.run,['ffmpeg','-v','error','-y','-framerate','3','-i',str(folder/'%05d.png'),'-an','-c:v','libx264','-crf','18','-vf','scale=trunc(iw/2)*2:trunc(ih/2)*2','-pix_fmt','yuv420p','-movflags','+faststart',str(path)],capture_output=True,text=True)
    if result.returncode:raise RuntimeError(result.stderr)
    MEDIA[a['id']]=(path.read_bytes(),'video/mp4');MEDIA['thumb-'+a['id']]=(subject_frames[-1][1],'image/png');report['checks']['real_browser_subject_frames']=len(subject_frames);return None
   raise RuntimeError('Unsupported local backend '+cmd)
  await context.expose_binding('__backend',backend)
  await page.goto('http://127.0.0.1:8791/desktop.html');await page.wait_for_timeout(1800)
  await pixels(path=str(OUT/'initial.png'));lib=page.frame_locator('#library')
  if args.inspect:
   print(await lib.locator('body').inner_text());print(report['errors']);await context.close();await browser.close();server.shutdown();return
  start=time.monotonic();report['performance_start']=await page.evaluate('performance.now()');frames=[];folder=OUT/'frames';folder.mkdir(exist_ok=True);filming=True
  async def film():
   while filming:
    t=time.monotonic()-start
    data=await pixels(type='jpeg',quality=96)
    fname=f'{len(frames):06d}.jpg';(folder/fname).write_bytes(data);frames.append({'file':fname,'t':t})
    await asyncio.sleep(.025)
  filming_task=asyncio.create_task(film())
  async def wait(seconds=8):await page.wait_for_timeout(seconds*1000 if not args.fast else min(seconds,.5)*1000)
  async def mark(name,delay=8):
   await wait(delay);stamp=time.monotonic()-start;report['scenes'].append({'name':name,'t':stamp});await pixels(path=str(OUT/f'proof-{len(report["scenes"]):02d}.png'));print(name,round(stamp,1),flush=True)
  async def move(x,y,duration=1.1):
   await page.mouse.move(x,y,steps=30);await asyncio.sleep(.15 if args.fast else duration)
  async def click(loc,pause=1):
   await loc.wait_for(state='visible');box=await loc.bounding_box();await move(box['x']+box['width']/2,box['y']+box['height']/2);await loc.click();await wait(pause)
  async def drag(x1,y1,x2,y2):
   await move(x1,y1);await page.mouse.down();await page.mouse.move(x2,y2,steps=36);await page.mouse.up();await wait(1)
  async def capture(mode):
   await page.evaluate('launchCapture()');o=page.frame_locator('#overlay');await click(o.get_by_role('button',name=mode,exact=True));await o.locator('img').first.evaluate('(im)=>im.decode()');return o
  try:
   await mark('素材库');await click(lib.get_by_role('button',name='设置',exact=True));await mark('设置')
   await click(lib.get_by_role('button',name='素材库',exact=True));await page.evaluate('showSubject()');await mark('工作笔记',3)
   o=await capture('截图');await drag(194,104,1088,567)
   await click(o.get_by_title('矩形 (R)',exact=True));await drag(236,156,951,248)
   await click(o.get_by_title('箭头 (A)',exact=True));await drag(1000,450,867,443)
   await click(o.get_by_title('文字 (T)',exact=True));await page.mouse.click(737,267)
   await o.locator('textarea').press_sequentially('先做好这一件',delay=180 if not args.fast else 10)
   await click(o.get_by_title('选择 (V)',exact=True));await mark('截图选区、图形与文字标注')
   await click(o.get_by_role('button',name=re.compile('完成.*Return')))
   await page.wait_for_function("state.assets.some(a=>a.kind==='image')")
   report['checks']['screenshot_saved']=True;await mark('截图完成与本地素材',4)
   o=await capture('文字');await drag(194,104,1090,641);await o.get_by_text('识别结果',exact=True).wait_for();await mark('整页文字识别：标题、清单和段落',12)
   await click(o.get_by_role('button',name='复制',exact=True));await wait(2)
   copied=await page.evaluate('state.copiedText');report['checks']['ocr_lines']=len(copied.splitlines());assert report['checks']['ocr_lines']>=12
   report['checks']['ocr_selection']={'x':194,'y':104,'width':896,'height':537}
   o=await capture('录屏');await drag(180,115,1110,566);await mark('区域录屏选区与选项')
   await click(o.get_by_role('button',name='开始录制',exact=True),pause=0)
   c=page.frame_locator('#countdown');await c.get_by_role('button',name='取消倒计时').wait_for();seen=[]
   await page.mouse.move(1180,630,steps=12)
   size=await c.locator('.kiri-countdown-ring').evaluate('(e)=>({width:e.offsetWidth,height:e.offsetHeight})')
   assert size=={'width':112,'height':112},size
   assert await c.locator('.kiri-countdown-cancel,kbd').count()==0
   assert await c.get_by_role('button',name='取消倒计时').inner_text()==''
   report['checks']['compact_ring_size']=112
   report['checks']['separate_cancel_row_absent']=True
   report['countdown_start']=time.monotonic()-start
   for n in [3,2,1]:
    await c.get_by_role('status',name=re.compile(str(n))).wait_for();await pixels(path=str(OUT/f'countdown-{n}.png'));seen.append(n)
   report['checks']['countdown_numerals']=seen
   ctrl=page.frame_locator('#control-panel');await ctrl.get_by_title('暂停录制',exact=True).wait_for();report['countdown_end']=time.monotonic()-start;await mark('黑色倒计时结束，录制控制条出现')
   subject=page.frame_locator('#subject');await click(subject.locator('input').nth(0));await wait(3)
   await click(ctrl.get_by_title('暂停录制',exact=True));await ctrl.get_by_text('已暂停',exact=True).wait_for();await mark('暂停录制');report['checks']['pause_shown']=True
   await click(ctrl.get_by_title('继续录制',exact=True));await click(subject.locator('input').nth(1));await wait(4)
   await click(subject.locator('input').nth(2));await click(subject.get_by_role('button',name='完成整理'));await mark('恢复录制，完成操作')
   await click(ctrl.get_by_title('停止并保存录屏',exact=True));await page.locator('#toast').wait_for();await mark('停止并保存',3)
   t=page.frame_locator('#toast');await click(t.get_by_role('button',name='打开录屏预览',exact=True))
   viewer=page.frame_locator('#viewer');await viewer.locator('video').wait_for();await viewer.locator('video').evaluate('(v)=>v.play()');await mark('回放刚保存的录制',10)
   report['checks']['video_playback_ready']=await viewer.locator('video').evaluate('(v)=>v.readyState>=2&&!v.error')
   await page.evaluate('openLibrary()');await page.frame_locator('#library').get_by_role('button',name='设置',exact=True).wait_for();await wait(1)
   report['ending_start']=time.monotonic()-start;await mark('截图和录屏都在素材库',6)
   report['checks']['no_recording_cancel_demo']=await page.evaluate("!state.calls.some(x=>x.c==='cancel_recording_flow')");assert report['checks']['no_recording_cancel_demo']
   report['checks']['saved_assets']=await page.evaluate('state.assets.map(x=>x.kind)');report['success']=not report['errors']
  except Exception as e:
   report['failure']=str(e);print('FAIL',str(e),flush=True);await pixels(path=str(OUT/'failure.png'))
   report['ui']=[{'url':f.url,'text':await f.evaluate('document.body.innerText'),'bridge':await f.evaluate('!!window.__TAURI_INTERNALS__'),'images':await f.evaluate('[...document.images].map(x=>({src:x.src,complete:x.complete,width:x.naturalWidth}))'),'controls':await f.evaluate("[...document.querySelectorAll('button,textarea,input')].map(x=>({tag:x.tagName,text:x.innerText,title:x.title,aria:x.getAttribute('aria-label'),placeholder:x.getAttribute('placeholder')}))")} for f in page.frames]
  filming=False;await filming_task;report['raw_seconds']=time.monotonic()-start;report['calls']=await page.evaluate('state.calls');report['frames']=frames
  if frames:report['dimensions']=list(Image.open(folder/frames[0]['file']).size)
  if report['dimensions']!=[1920,1080]:report.update(success=False,failure='Unexpected captured device resolution')
  (OUT/'report.json').write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf-8')
  await context.close();await browser.close()
 server.shutdown()
 if not report['success']:raise SystemExit(1)
if __name__=='__main__':
 ap=argparse.ArgumentParser();ap.add_argument('--inspect',action='store_true');ap.add_argument('--fast',action='store_true');asyncio.run(run(ap.parse_args()))
