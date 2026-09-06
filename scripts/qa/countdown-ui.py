"""Built countdown regression tests. Only native IPC is replaced in this harness."""
import argparse
import asyncio
import functools
import hashlib
import json
import os
import shutil
import subprocess
import tempfile
import threading
import time
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from playwright.async_api import async_playwright

OUT = Path('countdown-review')
OUT.mkdir(exist_ok=True)
SESSION = 'a6c9a5df-cf18-41ae-a435-72c837d226d1'
BRIDGE = '''(() => {
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
    def log_message(self, *args):
        pass

async def count(win, command):
    return await win.evaluate('(c)=>calls.filter(x=>x.command===c).length', command)

async def verify_layout(win, size=112):
    layout = await win.evaluate('''() => {
      const style = s => getComputedStyle(document.querySelector(s));
      const bounds = s => {const r=document.querySelector(s).getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height}};
      return {
        number: style('.kiri-countdown-number').color,
        fontSize: style('.kiri-countdown-number').fontSize,
        progress: style('.kiri-countdown-progress').stroke,
        strokeWidth: style('.kiri-countdown-progress').strokeWidth,
        track: style('.kiri-countdown-track').stroke,
        backdrop: style('.kiri-countdown').backgroundColor,
        disc: style('.kiri-countdown-disc').fill,
        ring: bounds('.kiri-countdown-ring'),
        action: bounds('.kiri-countdown-action'),
        visibleText: document.querySelector('.kiri-countdown').innerText.trim(),
        extraRow: document.querySelectorAll('.kiri-countdown-cancel,.kiri-countdown-stop,kbd').length
      };
    }''')
    assert layout['number'] == layout['progress'] == 'rgb(17, 17, 17)', layout
    assert layout['track'] == 'rgb(229, 229, 229)'
    assert layout['backdrop'] == 'rgba(0, 0, 0, 0)'
    assert layout['disc'] == 'rgba(255, 255, 255, 0.96)'
    assert layout['strokeWidth'] == '2.5px'
    assert layout['fontSize'] == ('44px' if size == 112 else '36px')
    assert layout['ring']['width'] == layout['ring']['height'] == size
    assert layout['action'] == layout['ring']
    assert layout['visibleText'] == '3' and layout['extraRow'] == 0
    viewport = win.viewport_size
    assert abs(layout['ring']['x'] + size/2 - viewport['width']/2) < 1
    assert abs(layout['ring']['y'] + size/2 - viewport['height']/2) < 1
    action = win.get_by_role('button', name='Cancel Countdown', exact=True)
    assert await action.get_attribute('aria-keyshortcuts') == 'Escape'
    assert await action.inner_text() == ''
    return layout

async def record_run(make_page):
    context, win = await make_page(viewport={'width':960,'height':540}, scale=2)
    try:
        await win.get_by_role('button', name='Cancel Countdown').wait_for()
        await win.evaluate('document.fonts.ready')
        await win.screenshot(path=str(OUT/'countdown-black.png'), scale='device')
        observed, frames = [], []
        with tempfile.TemporaryDirectory(prefix='kiri-countdown-') as directory:
            root = Path(directory)
            start = time.monotonic()
            await win.evaluate('releaseReady()')
            while time.monotonic() - start < 5:
                tick = await win.locator('.kiri-countdown-number').inner_text()
                if tick not in observed:
                    observed.append(tick)
                path = root/f'{len(frames):04d}.png'
                stamp = time.monotonic()
                await win.screenshot(path=str(path), scale='device')
                frames.append((path, stamp))
                if await count(win, 'begin_recording'):
                    break
                await asyncio.sleep(.025)
            assert observed == ['3','2','1'] and await count(win,'begin_recording') == 1
            listing = []
            for index, (path, stamp) in enumerate(frames):
                listing.append(f"file '{path.as_posix()}'")
                if index + 1 < len(frames):
                    listing.append(f'duration {frames[index+1][1]-stamp:.8f}')
            playlist = root/'frames.txt'
            playlist.write_text('\n'.join(listing)+'\n', encoding='utf-8')
            encoder = shutil.which('ffmpeg')
            if not encoder:
                import imageio_ffmpeg
                encoder = imageio_ffmpeg.get_ffmpeg_exe()
            target = OUT/'countdown-black.mp4'
            subprocess.run([encoder,'-hide_banner','-loglevel','error','-y',
                '-f','concat','-safe','0','-i',str(playlist),'-vf','fps=30','-an',
                '-c:v','libx264','-preset','fast','-crf','16','-pix_fmt','yuv420p',
                '-movflags','+faststart',str(target)], check=True)
        manifest = {'source_commit':os.environ.get('GITHUB_SHA'),
            'visual_source':'Actual built compact CountdownWindow; no separate cancel row',
            'native_ipc':'Isolated test stub; not native desktop capture',
            'size':[1920,1080],'logical_ring_size':112,'device_scale_factor':2,
            'speed_factor':1,'scene_cuts':0,'extra_holds':0,
            'observed_numerals':observed,'frames':len(frames),
            'observed_seconds':frames[-1][1]-frames[0][1],
            'sha256':hashlib.sha256(target.read_bytes()).hexdigest()}
        (OUT/'video-provenance.json').write_text(json.dumps(manifest,indent=2)+'\n')
    finally:
        await context.close()

async def main(record=False):
    server = ThreadingHTTPServer(('127.0.0.1',8765), functools.partial(Handler,directory='dist'))
    threading.Thread(target=server.serve_forever,daemon=True).start()
    results = []
    try:
        async with async_playwright() as p:
            browser = await p.chromium.launch(channel='chrome')
            async def page(kind='countdown', extra='', viewport=None, scale=1, reduced_motion='no-preference'):
                context = await browser.new_context(viewport=viewport or {'width':960,'height':640},
                    device_scale_factor=scale,reduced_motion=reduced_motion)
                await context.route('**/*',lambda route:route.continue_() if route.request.url.startswith('http://127.0.0.1:8765/') else route.abort())
                await context.add_init_script(script=BRIDGE+extra)
                win = await context.new_page()
                await win.goto(f'http://127.0.0.1:8765/?window={kind}&session={SESSION}')
                return context, win

            c,w = await page()
            await w.get_by_role('button',name='Cancel Countdown').wait_for()
            layout = await verify_layout(w)
            results.append('112px centered black ring, 44px numeral, thin stroke and no extra button row')
            await w.wait_for_timeout(3300)
            assert await count(w,'begin_recording') == 0
            await w.screenshot(path=str(OUT/'countdown-actual.png'))
            await w.evaluate('releaseReady()')
            await w.wait_for_function("document.activeElement.matches('.kiri-countdown')")
            assert await w.locator('.kiri-countdown-action').evaluate("e=>getComputedStyle(e).outlineStyle") == 'none'
            results.append('readiness focuses the surface without an automatic ring halo')
            await w.get_by_role('status',name='Recording starts in 2').wait_for()
            await w.get_by_role('status',name='Recording starts in 1').wait_for()
            await w.wait_for_timeout(1300)
            assert await count(w,'begin_recording') == 1
            assert await w.evaluate("calls.find(c=>c.command==='begin_recording').args") == {'sessionId':SESSION}
            results.append('ready -> 3 -> 2 -> 1 -> one session-bound native start')
            await c.close()
            for key in ['click','Escape','Enter','Space']:
                c,w = await page()
                action = w.get_by_role('button',name='Cancel Countdown')
                await action.wait_for()
                await w.evaluate('releaseReady()')
                await w.wait_for_function("document.activeElement.matches('.kiri-countdown')")
                if key == 'click':
                    await action.click()
                elif key == 'Escape':
                    await w.keyboard.press('Escape')
                else:
                    await w.keyboard.press('Tab')
                    assert await action.evaluate('(e)=>document.activeElement===e')
                    assert await action.evaluate('(e)=>getComputedStyle(e).outlineStyle') == 'solid'
                    await w.keyboard.press(key)
                await w.wait_for_timeout(3200)
                assert await count(w,'begin_recording') == 0
                assert await count(w,'cancel_recording_flow') == 1
                results.append(key+' cancels before expiry; ring remains keyboard accessible')
                await c.close()
            c,w = await page(extra='window.failCancel=true;')
            action = w.get_by_role('button',name='Cancel Countdown')
            await action.click()
            await w.get_by_role('alert').wait_for()
            await action.click()
            await w.evaluate('releaseReady()')
            await w.wait_for_timeout(3300)
            assert await count(w,'cancel_recording_flow') == 2 and await count(w,'begin_recording') == 0
            results.append('failed cancellation remains retryable without restarting')
            await c.close()
            c,w = await page(extra='window.failReady=true;')
            await w.get_by_role('alert').wait_for()
            await w.keyboard.press('Escape')
            assert await count(w,'cancel_recording_flow') == 1 and await count(w,'begin_recording') == 0
            results.append('readiness error remains visible and cancellable')
            await c.close()
            c,w = await page('control-panel')
            await w.get_by_role('button',name='Cancel',exact=True).wait_for()
            assert await w.get_by_role('button',name='Cancel',exact=True).is_enabled()
            results.append('late control panel hydrates its cancel action')
            await c.close()
            for viewport,size in [({'width':320,'height':320},112),({'width':280,'height':220},88)]:
                c,w = await page(viewport=viewport,reduced_motion='reduce')
                await w.get_by_role('button',name='Cancel Countdown').wait_for()
                await verify_layout(w,size)
                assert await w.locator('.kiri-countdown-action').evaluate('(e)=>getComputedStyle(e).transitionDuration') == '0s'
                await c.close()
            results.append('small-window layouts stay centered and honor reduced motion')
            if record:
                await record_run(page)
                results.append('recorded actual compact renderer at device resolution')
            await browser.close()
    finally:
        server.shutdown()
        server.server_close()
    (OUT/'ui-tests.json').write_text(json.dumps({'passed':results,'layout':layout,'native':False},indent=2)+'\n')

if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--record',action='store_true')
    asyncio.run(main(record=parser.parse_args().record))
