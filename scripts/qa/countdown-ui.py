"""UI regression against the built app; mocks IPC only, never changes shipped code."""
import asyncio, functools, json, threading
from pathlib import Path
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from playwright.async_api import async_playwright

OUT=Path('countdown-review');OUT.mkdir(exist_ok=True)
SESSION='a6c9a5df-cf18-41ae-a435-72c837d226d1'
BRIDGE='''(() => {
window.calls=[];window.ready=false;window.releaseReady=()=>{window.ready=true};
const callbacks=new Map();let serial=0;
window.__TAURI_INTERNALS__={
metadata:{currentWindow:{label:'countdown'},currentWebview:{label:'countdown',windowLabel:'countdown'}},
transformCallback:fn=>{callbacks.set(++serial,fn);return serial},unregisterCallback:id=>callbacks.delete(id),
convertFileSrc:p=>p,invoke:async(c,a={})=>{
window.calls.push({command:c,args:a});
if(c==='get_language'||c==='get_locale')return 'en';
if(c==='recording_countdown_ready'){
 if(window.failReady)throw Error('controlled readiness failure');
 while(!window.ready)await new Promise(r=>setTimeout(r,10));return;
}
if(c==='cancel_recording_flow'&&window.failCancel){window.failCancel=false;throw Error('controlled cancellation failure')}
if(c==='get_recording_state')return {isStarting:true,isRecording:false,isPaused:false,isTransitioning:false,isFinalizing:false,elapsed:0,elapsedLabel:'00:00'};
if(c==='plugin:event|listen')return ++serial;
return null;
}};window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:()=>{}};
})();'''
class Handler(SimpleHTTPRequestHandler):
 def log_message(self,*args):pass
async def main():
 server=ThreadingHTTPServer(('127.0.0.1',8765),functools.partial(Handler,directory='dist'))
 threading.Thread(target=server.serve_forever,daemon=True).start();results=[]
 async with async_playwright() as p:
  browser=await p.chromium.launch(channel='chrome')
  async def page(kind='countdown',extra=''):
   context=await browser.new_context(viewport={'width':960,'height':640})
   await context.route('**/*',lambda route:route.continue_() if route.request.url.startswith('http://127.0.0.1:8765/') else route.abort())
   await context.add_init_script(script=BRIDGE+extra)
   win=await context.new_page();await win.goto(f'http://127.0.0.1:8765/?window={kind}&session={SESSION}')
   return context,win
  async def count(win,cmd):return await win.evaluate('(c)=>calls.filter(x=>x.command===c).length',cmd)
  c,w=await page();await w.get_by_role('button',name='Cancel Countdown').wait_for()
  await w.wait_for_timeout(3300);assert await count(w,'begin_recording')==0
  await w.screenshot(path=str(OUT/'countdown-actual.png'))
  await w.evaluate('releaseReady()');await w.get_by_role('status',name='Recording starts in 2').wait_for()
  await w.get_by_role('status',name='Recording starts in 1').wait_for();await w.wait_for_timeout(1300)
  assert await count(w,'begin_recording')==1
  args=await w.evaluate("calls.find(c=>c.command==='begin_recording').args")
  assert args=={'sessionId':SESSION};results.append('ready -> 3 -> 2 -> 1 -> one native start');await c.close()
  for key in ['click','Escape']:
   c,w=await page();button=w.get_by_role('button',name='Cancel Countdown');await button.wait_for();await w.evaluate('releaseReady()');await w.wait_for_timeout(200)
   if key=='click':await button.click()
   else:await w.keyboard.press('Escape')
   await w.wait_for_timeout(3200);assert await count(w,'begin_recording')==0;assert await count(w,'cancel_recording_flow')==1
   results.append(key+' cancels before expiry');await c.close()
  c,w=await page(extra='window.failCancel=true;');button=w.get_by_role('button',name='Cancel Countdown');await button.click()
  await w.get_by_role('alert').wait_for();await button.click();await w.evaluate('releaseReady()');await w.wait_for_timeout(3300)
  assert await count(w,'cancel_recording_flow')==2;assert await count(w,'begin_recording')==0;results.append('failed cancellation can retry without restarting');await c.close()
  c,w=await page('control-panel');await w.get_by_role('button',name='Cancel',exact=True).wait_for()
  assert await w.get_by_role('button',name='Cancel',exact=True).is_enabled();results.append('late control panel hydrates its cancel action');await c.close()
  await browser.close()
 server.shutdown();(OUT/'ui-tests.json').write_text(json.dumps({'passed':results,'native':False},indent=2))
asyncio.run(main())
