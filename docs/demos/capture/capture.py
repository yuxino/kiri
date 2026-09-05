"""Documentation capture only. App bundles remain unmodified; native boundaries use fixtures."""
import asyncio
import functools
import hashlib
import html
import json
import re
import subprocess
import threading
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from playwright.async_api import async_playwright

ROOT = Path(__file__).resolve().parent
PROJECTS = ['kiri', 'mimi', 'satori', 'viva', 'tick', 'wnacg']
OUT = ROOT / 'recordings'
OUT.mkdir(exist_ok=True)
TITLES = {'kiri':'Kiri', 'mimi':'Mimi', 'satori':'Satori', 'viva':'Viva', 'tick':'Tick', 'wnacg':'WNACG'}
LIMITS = {
 'kiri':'Annotation UI · original sample image · no OS screen capture',
 'mimi':'Settings UI · built-in browser preview · no live transcription',
 'satori':'PDF reading UI · original local sample · no AI answers',
 'viva':'Markdown UI · in-memory sample files · no native file access',
 'tick':'Task configuration UI · no task saved or executed',
 'wnacg':'Reading UI · original local pages · no upstream content or AI',
}
CAPTIONS = {
 'kiri':('圈出重点，加一句说明。','Mark what matters. Add a little context.'),
 'mimi':('把字幕调成习惯的样子。','Make the subtitle controls your own.'),
 'satori':('从眼前这一页，慢慢读下去。','Turn a page. Find your own pace.'),
 'viva':('写下来，再换个角度看看。','Write a note. See it another way.'),
 'tick':('设定时间，整理一件重复的小事。','Give a recurring task a place in your day.'),
 'wnacg':('按习惯翻页，按心情换布局。','A reading layout that follows your pace.'),
}

class Handler(SimpleHTTPRequestHandler):
    def log_message(self, *args): pass
    def do_GET(self):
        if self.path == '/demo.html':
            repo=PROJECTS[self.server.server_port-8730]
            q='?window=editor&id=demo' if repo=='kiri' else '#aid=1001' if repo=='wnacg' else ''
            cn,en=CAPTIONS[repo]
            text=f'''<!doctype html><meta charset="utf-8"><title>{TITLES[repo]} interface demo</title>
<style>*{{box-sizing:border-box}}body{{margin:0;background:#f5f4f8;color:#342f40;font-family:Arial,"Noto Sans CJK SC",sans-serif}}header{{height:64px;margin:0 32px;display:flex;align-items:center;justify-content:space-between}}b{{font-size:25px;letter-spacing:-.5px}}.badge{{font-size:12px;background:#ece7f4;padding:8px 13px;border:1px solid #dfd6eb;border-radius:20px;color:#776585}}iframe{{display:block;margin:0 32px;width:1216px;height:748px;border:1px solid #ddd9e4;border-radius:10px;background:white;box-shadow:0 8px 22px #36304410}}footer{{margin:18px 34px 0;display:flex;align-items:center;justify-content:space-between}}#cn{{font-size:17px;font-weight:600}}#en{{font-size:12px;color:#88818f;margin-top:5px}}.limit{{font-size:10px;color:#8e8696;max-width:370px;text-align:right;line-height:1.6}}</style>
<header><b>{TITLES[repo]}</b><span class="badge">UI DEMO · SAMPLE DATA / 界面演示 · 示例数据</span></header>
<iframe id="app" src="/{q}"></iframe><footer><div><div id="cn">{html.escape(cn)}</div><div id="en">{html.escape(en)}</div></div><div class="limit">{html.escape(LIMITS[repo])}</div></footer>'''
            data=text.encode();self.send_response(200);self.send_header('Content-Type','text/html; charset=utf-8');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
        else:super().do_GET()
    def translate_path(self, path):
        if path.startswith('/__demo__/'):
            return str(ROOT/'fixtures'/path.split('?')[0].rsplit('/',1)[-1])
        return super().translate_path(path)

POINTER = """() => {
const dot=document.createElement('div');dot.id='documentation-pointer';
dot.style.cssText='position:fixed;left:-40px;top:-40px;width:15px;height:15px;border-radius:50%;background:#8972b1aa;border:2px solid white;box-shadow:0 0 0 1px #716086;transform:translate(-50%,-50%);pointer-events:none;z-index:2147483647';
document.body.append(dot);document.addEventListener('mousemove',e=>{dot.style.left=e.clientX+'px';dot.style.top=e.clientY+'px'});document.addEventListener('mousedown',()=>dot.style.background='#534064');document.addEventListener('mouseup',()=>dot.style.background='#8972b1aa');
}"""

async def record(repo,browser):
    folder=OUT/repo;folder.mkdir(exist_ok=True)
    port=8730+PROJECTS.index(repo)
    ctx=await browser.new_context(viewport={'width':1280,'height':900},locale='en-US',color_scheme='light',record_video_dir=str(folder/'raw'),record_video_size={'width':1280,'height':900})
    async def route(request):
        url=request.request.url
        if url.startswith(f'http://127.0.0.1:{port}/'):
            await request.continue_()
        elif re.fullmatch(r'https://img\.qy0\.ru/demo/page-[123]\.png',url):
            await request.fulfill(path=str(ROOT/'fixtures'/url.rsplit('/',1)[-1]),content_type='image/png')
        else:await request.abort()
    await ctx.route('**/*',route)
    bridge=(ROOT/'bridge.js').read_text().replace('__PROJECT__',repo)
    bridge += "\nif (window.__TAURI_INTERNALS__) {globalThis.isTauri=true;}\n"
    await ctx.add_init_script(script=bridge)
    page=await ctx.new_page();page.set_default_timeout(9000)
    errors=[];steps=[]
    page.on('pageerror',lambda error:errors.append(str(error)))
    state={'project':repo,'source_commit':(ROOT/'dist'/f'demo-frontend-{repo}'/'source-commit.txt').read_text().strip(),'capture':'Unmodified production frontend in Chrome, documentation-only native-boundary fixtures','limitations':LIMITS[repo],'steps':steps,'success':False,'browser':browser.version}
    start=0
    async def caption(cn,en):
        steps.append({'zh':cn,'en':en})
        await page.locator('#cn').evaluate('(el,text)=>el.textContent=text',cn)
        await page.locator('#en').evaluate('(el,text)=>el.textContent=text',en)
    async def click(locator,pause=1300):
        await locator.scroll_into_view_if_needed()
        box=await locator.bounding_box()
        if box:await page.mouse.move(box['x']+box['width']/2,box['y']+box['height']/2,steps=16)
        await locator.click();await page.wait_for_timeout(pause)
    async def shot(name):await page.screenshot(path=str(folder/(name+'.png')))
    try:
        await page.goto(f'http://127.0.0.1:{port}/demo.html')
        frame=page.frame_locator('#app')
        await frame.locator('body').wait_for()
        await page.wait_for_timeout(2200)
        app=page.frames[1]
        await app.evaluate(POINTER)
        start=await page.evaluate('performance.now()/1000')
        await page.wait_for_timeout(1000)
        if repo=='kiri':
            canvas=frame.locator('canvas').first
            await canvas.wait_for();box=await canvas.bounding_box()
            async def drag(x1,y1,x2,y2):
                await page.mouse.move(box['x']+x1*box['width'],box['y']+y1*box['height'],steps=16)
                await page.mouse.down();await page.mouse.move(box['x']+x2*box['width'],box['y']+y2*box['height'],steps=35);await page.mouse.up();await page.wait_for_timeout(1200)
            await caption('先圈出最想保留的那一点。','Highlight one detail with a rectangle.')
            await click(frame.get_by_title('Rectangle (R)'));await drag(.065,.247,.49,.51);await shot('01')
            await caption('用箭头，把视线引过去。','Add an arrow to guide the eye.')
            await click(frame.get_by_title('Arrow (A)'));await drag(.67,.515,.535,.43);await shot('02')
            await caption('再留下一句自己的说明。','Add a short note, then review it.')
            await click(frame.get_by_title('Text (T)'))
            await page.mouse.click(box['x']+.56*box['width'],box['y']+.53*box['height'])
            await frame.get_by_placeholder('Type something…').press_sequentially('Start small.',delay=95)
            await click(frame.get_by_title('Select (V)'),1500);await shot('poster')
            await caption('随时撤销，随时重来。','Undo and redo without starting over.')
            await click(frame.get_by_title('Undo (⌘Z)'));await click(frame.get_by_title('Redo (⇧⌘Z)'));await shot('03')
        elif repo=='mimi':
            await caption('选择主要语言，保留熟悉的翻译方向。','Choose the source and translation language.')
            await click(frame.get_by_role('button',name='Japanese',exact=True))
            await frame.get_by_label('Translate To',exact=True).select_option('en');await page.wait_for_timeout(1200)
            await caption('字号、对齐，都按自己的习惯来。','Adjust subtitle size and alignment.')
            slider=frame.get_by_label('Subtitle Size',exact=True)
            await click(slider,250);await slider.press('End');await page.wait_for_timeout(1200)
            await click(frame.get_by_label('Align Left',exact=True));await click(frame.get_by_label('Align Center',exact=True));await shot('01')
            await caption('需要更少干扰时，切换沉浸模式。','Explore immersive mode in the settings preview.')
            await click(frame.get_by_label('Immersive Mode',exact=True));await shot('poster');await page.wait_for_timeout(1600)
            await click(frame.get_by_label('Immersive Mode',exact=True));await shot('02')
        elif repo=='viva':
            await caption('打开一份示例笔记。','Open a sample Markdown workspace.')
            await click(frame.get_by_role('button',name='Open folder',exact=True))
            await click(frame.get_by_text('Weekend.md',exact=True).first);await shot('01')
            await caption('在源码里补上一句，再看实时预览。','Write a line in Source, then switch to Split.')
            await click(frame.get_by_label('Source',exact=True))
            editor=frame.get_by_role('textbox',name='Editing Weekend.md',exact=True)
            await editor.click();await editor.press('Control+Home');await editor.press('End');await editor.press('Enter');await editor.press('Enter')
            await editor.press_sequentially('One useful thing is enough for today.',delay=60)
            await click(frame.get_by_label('Split',exact=True));await shot('poster');await page.wait_for_timeout(1600)
            await caption('切回 Live，把注意力留给文字。','Switch to Live and keep the focus on your words.')
            await click(frame.get_by_label('Live',exact=True));await shot('02');await page.wait_for_timeout(1800)
        elif repo=='satori':
            await caption('带一本示例 PDF 进来。','Open an original three-page sample PDF.')
            await click(frame.get_by_role('button',name='打开 PDF…',exact=True).first,2400)
            await frame.get_by_title('下一页',exact=True).wait_for()
            await frame.locator('canvas').first.wait_for();await shot('01')
            await caption('翻一页，放大看看细节。','Turn a page and zoom in on the details.')
            await click(frame.get_by_title('下一页',exact=True));await click(frame.get_by_title('放大',exact=True));await shot('02')
            await click(frame.get_by_title('回到适合窗口',exact=True))
            await caption('单页、双页，随阅读习惯切换。','Switch between single-page and spread layouts.')
            await click(frame.get_by_role('button',name='双页',exact=True));await shot('poster');await page.wait_for_timeout(1700)
            await caption('打开目录，回到想读的章节。','Use the embedded outline to find your place.')
            await click(frame.get_by_title('章节跳转',exact=True));await shot('03');await page.wait_for_timeout(1700)
        elif repo=='tick':
            await caption('手动填写一个定时任务。','Start with a manually configured task.')
            await click(frame.get_by_role('button',name='手动填写',exact=True))
            await frame.get_by_placeholder('每日同步').fill('每日笔记整理')
            await frame.get_by_placeholder('可选备注').fill('示例任务：只演示配置，不运行')
            await page.wait_for_timeout(1500);await shot('01')
            await caption('设成每小时一次，时间一眼就明白。','Choose an hourly interval.')
            await click(frame.get_by_text('循环间隔',exact=True));await click(frame.get_by_role('button',name='1 小时',exact=True))
            await caption('填入一小段脚本，保存前仍可检查。','Review the script before saving. Nothing runs here.')
            content=frame.locator('.cm-content[contenteditable=true]').first
            await content.scroll_into_view_if_needed();await content.click();await page.keyboard.press('Control+A')
            await page.keyboard.type('// Demo only — no files are changed.\nconsole.log("A little room for good ideas");',delay=40)
            await page.wait_for_timeout(1500);await shot('poster');await page.wait_for_timeout(1800)
            await caption('演示到这里，不创建系统任务。','Cancel the example without creating a system task.')
            await click(frame.get_by_role('button',name=re.compile(r'^取\s*消$')).last);await shot('02')
        else:
            await frame.locator('.reader-page img').first.wait_for(state='visible')
            await caption('先按连续模式慢慢读。','Read original sample pages in continuous mode.')
            await page.mouse.move(790,520);await page.mouse.wheel(0,530);await page.wait_for_timeout(1700);await shot('01')
            await caption('打开阅读设置，换成单页。','Choose a single-page layout.')
            await click(frame.get_by_label('阅读设置',exact=True).first)
            await click(frame.get_by_role('button',name='单页',exact=True));await page.keyboard.press('Escape');await page.wait_for_timeout(1200);await shot('02')
            await caption('再试试双页，像翻开一本书。','Try a two-page spread.')
            await click(frame.get_by_label('阅读设置',exact=True).first)
            await click(frame.get_by_role('button',name=re.compile('^双页')));await page.keyboard.press('Escape');await page.wait_for_timeout(1500);await shot('poster')
            await page.mouse.click(600,600);await page.keyboard.press('ArrowRight');await page.wait_for_timeout(1600);await shot('03')
        await page.wait_for_timeout(1700)
        state['success']=True
    except Exception as error:
        state['failure']=str(error);await shot('failure')
    state['javascript_errors']=errors
    try:
        app=page.frames[1]
        state['final_ui']=await app.evaluate("""() => ({text:document.body.innerText,controls:[...document.querySelectorAll('button,input,textarea,select,[contenteditable=true]')].map(e=>({tag:e.tagName,text:e.innerText,title:e.getAttribute('title'),aria:e.getAttribute('aria-label'),placeholder:e.getAttribute('placeholder')})),calls:window.__demoCalls})""")
    except Exception as error:state['inspection_error']=str(error)
    path=await page.video.path();await ctx.close()
    if state['success']:
        result=await asyncio.to_thread(subprocess.run,['ffmpeg','-hide_banner','-loglevel','error','-y','-ss',str(max(0,start-.15)),'-i',str(path),'-an','-c:v','libx264','-preset','fast','-crf','23','-pix_fmt','yuv420p','-movflags','+faststart','-threads','2',str(folder/'demo.mp4')],capture_output=True,text=True)
        if result.returncode:state.update(success=False,encoding_error=result.stderr)
        else:
            state['sha256']=hashlib.sha256((folder/'demo.mp4').read_bytes()).hexdigest()
            state['duration']=float(subprocess.check_output(['ffprobe','-v','quiet','-show_entries','format=duration','-of','csv=p=0',str(folder/'demo.mp4')],text=True).strip())
    (folder/'provenance.json').write_text(json.dumps(state,ensure_ascii=False,indent=2),encoding='utf-8')
    print(repo, 'OK' if state['success'] else state.get('failure',state.get('encoding_error')))
    return state['success']

async def main():
    servers=[]
    for i,repo in enumerate(PROJECTS):
        server=ThreadingHTTPServer(('127.0.0.1',8730+i),functools.partial(Handler,directory=str(ROOT/'dist'/f'demo-frontend-{repo}')))
        threading.Thread(target=server.serve_forever,daemon=True).start();servers.append(server)
    async with async_playwright() as pw:
        # Use the runner's current stable Chrome for PDF.js's modern JavaScript APIs.
        browser=await pw.chromium.launch(channel='chrome')
        results=await asyncio.gather(*(record(repo,browser) for repo in PROJECTS))
        await browser.close()
    for server in servers:server.shutdown()
    if not all(results):raise SystemExit('Some scenes failed. See per-project diagnostics; do not publish failed clips.')

if __name__=='__main__':asyncio.run(main())
