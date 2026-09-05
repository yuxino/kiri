"""Expanded documentation walkthroughs. No changes to application code or credentials."""
import asyncio
import functools
import hashlib
import html
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import threading
from http.server import ThreadingHTTPServer
from playwright.async_api import async_playwright

ROOT=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('original_capture',ROOT/'capture.py')
base=importlib.util.module_from_spec(spec);spec.loader.exec_module(base)
PROJECTS=base.PROJECTS
OUT=ROOT/'expanded';OUT.mkdir(exist_ok=True)
SPEED=10
HOLD=.8

class Handler(base.Handler):
    def do_GET(self):
        if self.path!='/demo.html':return super().do_GET()
        repo=PROJECTS[self.server.server_port-8730]
        query='?window=editor&id=demo' if repo=='kiri' else '#aid=1001' if repo=='wnacg' else ''
        data=f'''<!doctype html><meta charset="utf-8"><title>{repo} feature tour</title>
<style>*{{box-sizing:border-box}}body{{margin:0;background:#f5f4f8;color:#342f40;font-family:Arial,"Noto Sans CJK SC",sans-serif}}header{{height:64px;margin:0 32px;display:flex;align-items:center;justify-content:space-between}}b{{font-size:25px}}.badge{{font-size:12px;background:#ece7f4;padding:8px 13px;border:1px solid #dfd6eb;border-radius:20px;color:#776585}}iframe{{display:block;margin:0 32px;width:1216px;height:748px;border:1px solid #ddd9e4;border-radius:10px;background:white;box-shadow:0 8px 22px #36304410}}footer{{margin:17px 34px 0;display:flex;align-items:center;justify-content:space-between}}#cn{{font-size:18px;font-weight:600}}#en{{font-size:12px;color:#88818f;margin-top:5px}}.limit{{font-size:10px;color:#8e8696;max-width:370px;text-align:right;line-height:1.6}}</style>
<header><b>{base.TITLES[repo]}</b><span class="badge">10× ACTIONS · RESULT HOLDS / 操作快放 · 示例数据</span></header>
<iframe id="app" src="/{query}"></iframe><footer><div><div id="cn">更多功能，一次看完。</div><div id="en">A closer look, at a faster pace.</div></div><div class="limit">{html.escape(base.LIMITS[repo])}</div></footer>'''.encode()
        self.send_response(200);self.send_header('Content-Type','text/html; charset=utf-8');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)

async def record(repo,browser):
    folder=OUT/repo;folder.mkdir(exist_ok=True)
    port=8730+PROJECTS.index(repo)
    context=await browser.new_context(viewport={'width':1280,'height':900},locale='en-US',color_scheme='light',record_video_dir=str(folder/'raw'),record_video_size={'width':1280,'height':900})
    async def route(rt):
        if rt.request.url.startswith(f'http://127.0.0.1:{port}/'):await rt.continue_()
        elif re.fullmatch(r'https://img\.qy0\.ru/demo/page-[123]\.png',rt.request.url):await rt.fulfill(path=str(ROOT/'fixtures'/rt.request.url.rsplit('/',1)[-1]),content_type='image/png')
        else:await rt.abort()
    await context.route('**/*',route)
    await context.add_init_script(script=(ROOT/'bridge.js').read_text().replace('__PROJECT__',repo).replace("localStorage.setItem('mimi-ui-language', 'en');", "if (!localStorage.getItem('mimi-ui-language')) localStorage.setItem('mimi-ui-language', 'en');")+"\nif (window.__TAURI_INTERNALS__) globalThis.isTauri=true;")
    page=await context.new_page();page.set_default_timeout(7000)
    errors=[];page.on('pageerror',lambda e:errors.append(str(e)))
    scenes=[];actions=[]
    state={'project':repo,'source_commit':(ROOT/'dist'/f'demo-frontend-{repo}'/'source-commit.txt').read_text().strip(),'capture':'Unmodified production frontend in Chrome with isolated local sample data and mocked native APIs','limitations':base.LIMITS[repo],'browser':browser.version,'speed_factor':SPEED,'result_hold_seconds':HOLD,'scenes':scenes,'actions':actions,'success':False}
    start=0
    async def now():return await page.evaluate('performance.now()/1000')
    async def caption(cn,en):
        await page.locator('#cn').evaluate('(el,s)=>el.textContent=s',cn)
        await page.locator('#en').evaluate('(el,s)=>el.textContent=s',en)
    async def click(loc):
        await loc.scroll_into_view_if_needed();box=await loc.bounding_box()
        if box:await page.mouse.move(box['x']+box['width']/2,box['y']+box['height']/2,steps=30)
        name=await loc.get_attribute('aria-label') or await loc.get_attribute('title') or (await loc.inner_text())[:80]
        await loc.click();actions.append(name);await page.wait_for_timeout(260)
    async def mark(cn,en):
        await caption(cn,en);await page.wait_for_timeout(1100)
        end=await now();begin=scenes[-1]['end'] if scenes else start
        name=f'{len(scenes)+1:02}.png';await page.screenshot(path=str(folder/name))
        scenes.append({'zh':cn,'en':en,'start':begin,'end':end,'poster':name})
    async def type_in(loc,text):
        await click(loc);await loc.fill('');await loc.press_sequentially(text,delay=70);actions.append('Type: '+text)
    try:
        await page.goto(f'http://127.0.0.1:{port}/demo.html');await page.wait_for_timeout(1800)
        f=page.frame_locator('#app');app=page.frames[1];await app.evaluate(base.POINTER);start=await now()
        if repo=='kiri':
            canvas=f.locator('canvas').first;await canvas.wait_for();box=await canvas.bounding_box()
            async def drag(a,b,c,d):
                await page.mouse.move(box['x']+a*box['width'],box['y']+b*box['height'],steps=25)
                await page.mouse.down();await page.mouse.move(box['x']+c*box['width'],box['y']+d*box['height'],steps=70);await page.mouse.up();actions.append('Draw or move canvas region');await page.wait_for_timeout(280)
            await click(f.get_by_title('Rectangle (R)'));await drag(.062,.246,.493,.51);await mark('矩形标注：先圈出重点','01 / Rectangle annotation')
            await click(f.get_by_label('Line',exact=True));await f.get_by_label('Line',exact=True).press('Home');await f.get_by_label('Line',exact=True).press('ArrowRight');await f.get_by_label('Line',exact=True).press('ArrowRight');await mark('调整接下来绘制的线宽','02 / Adjust the annotation style')
            await click(f.get_by_title('Arrow (A)'));await drag(.81,.52,.68,.40);await mark('箭头：把视线引向关键细节','03 / Draw an arrow')
            await click(f.get_by_title('Text (T)'));await page.mouse.click(box['x']+.55*box['width'],box['y']+.54*box['height']);await type_in(f.get_by_placeholder('Type something…'),'Start small.');await click(f.get_by_title('Select (V)'));await mark('文字标注：补上一句说明','04 / Add an editable text annotation')
            await click(f.get_by_title('Pen (P)'));await drag(.094,.76,.34,.76);await mark('画笔：随手划一道重点','05 / Freehand pen')
            await click(f.get_by_title('Mosaic (M)'));await click(f.get_by_title('Strong',exact=True));await drag(.135,.421,.43,.421);await mark('马赛克：遮住不想展示的区域','06 / Pixel mosaic with adjustable strength')
            await click(f.get_by_title('Gaussian blur',exact=True));await drag(.57,.725,.83,.725);await mark('也可以换成柔和的模糊','07 / Switch to the blur brush')
            await click(f.get_by_title('Undo (⌘Z)'));await mark('一步撤销，继续修改','08 / Undo an edit')
            await click(f.get_by_title('Redo (⇧⌘Z)'));await click(f.get_by_title('Select (V)'));await mark('重做后，回到完整标注','09 / Redo and review')
            await click(f.get_by_title('Crop (C)'));await mark('裁剪工具也在同一处','10 / Inspect crop controls without exporting')
        elif repo=='mimi':
            await click(f.get_by_role('button',name='Japanese',exact=True));await f.get_by_label('Translate To',exact=True).select_option('en');actions.append('Select translation target: English');await mark('日语输入，英语字幕','01 / Source and target languages')
            await click(f.get_by_role('button',name='English',exact=True));await f.get_by_label('Translate To',exact=True).select_option('zh');await mark('随时切换翻译方向','02 / Change the language pair')
            mode=f.get_by_label('Translation Mode',exact=True);values=await mode.locator('option').evaluate_all('(xs)=>xs.map(x=>x.value)');await mode.select_option(values[-1]);actions.append('Switch translation quality mode');await mark('按需要选择翻译模式','03 / Translation quality controls')
            size=f.get_by_label('Subtitle Size',exact=True);await click(size);await size.press('End');await mark('字号放大，远一点也能看清','04 / Enlarge subtitles')
            await click(f.get_by_label('Align Left',exact=True));await click(f.get_by_label('Align Right',exact=True));await click(f.get_by_label('Align Center',exact=True));await mark('左、中、右，选习惯的对齐方式','05 / Three subtitle alignments')
            await click(f.get_by_label('Immersive Mode',exact=True));await click(f.get_by_label('Lock Subtitle Position',exact=True));await mark('沉浸显示，也能锁定字幕位置','06 / Immersive and position-lock controls')
            await click(f.get_by_role('button',name='Translation Service',exact=True));await mark('翻译服务集中管理','07 / Translation service settings')
            summary=f.locator('summary').filter(has_text='Manage').first;await click(summary);await click(f.get_by_role('button',name='Add Configuration',exact=True));await mark('查看可添加的实时服务','08 / Explore supported provider choices')
            await click(f.get_by_role('button',name='Cancel',exact=True));await click(f.get_by_role('button',name='General',exact=True));await mark('通用设置与界面语言','09 / General settings')
            await f.get_by_label('Interface Language',exact=True).select_option('zh');actions.append('Switch UI language to Chinese');await f.get_by_role('heading',name='设置',exact=True).wait_for();await mark('中英文界面，一键切换','10 / Switch the interface language')
        elif repo=='viva':
            await click(f.get_by_role('button',name='Open folder',exact=True));await click(f.get_by_text('Weekend.md',exact=True).first);await mark('打开文件夹，开始一份笔记','01 / Open a Markdown workspace')
            await click(f.get_by_label('Source',exact=True));editor=f.get_by_role('textbox',name='Editing Weekend.md',exact=True);await click(editor);await editor.press('Control+End');await editor.press_sequentially('\n\n## A small next step\n\nMake one useful thing today.\n',delay=65);actions.append('Edit Markdown source');await mark('源码里写下一个新想法','02 / Edit the Markdown source')
            await click(f.get_by_label('Split',exact=True));await mark('边写边看，源码与排版并排','03 / Split editor and preview')
            await click(f.get_by_label('Preview',exact=True));await mark('只看排版后的页面','04 / Full-page preview')
            await click(f.get_by_label('Live',exact=True));await mark('切回 Live，继续在页面上写','05 / Live editing view')
            await click(f.get_by_label('Outline',exact=True));await mark('目录自动跟随文档结构','06 / Document outline')
            await click(f.get_by_label('Files',exact=True));await click(f.get_by_text('Ideas.md',exact=True).first);await mark('另一份文档，留在另一个标签','07 / Work across multiple tabs')
            await click(f.get_by_role('tab',name=re.compile('^Weekend\.md')));await click(f.get_by_label('Source',exact=True));await f.get_by_role('textbox',name='Editing Weekend.md',exact=True).click();await page.keyboard.press('Control+f');await type_in(f.get_by_role('searchbox',name='Find in document',exact=True),'small');await mark('在当前文档里查找关键词','08 / Find in the current document')
            await click(f.get_by_label('Close find and replace',exact=True));await click(f.get_by_label('Focus mode',exact=True));await mark('进入专注模式，暂时收起其他东西','09 / Focus mode')
            await page.keyboard.press('Control+Shift+Enter');actions.append('Exit focus mode shortcut');await click(f.get_by_label('Appearance and background',exact=True));await mark('外观与背景，在这里调整','10 / Appearance controls')
            await click(f.get_by_role('radio',name='Dark',exact=True));await mark('也提供深色工作区','11 / Dark appearance')
            await click(f.get_by_role('radio',name='Light',exact=True));await click(f.get_by_label('Return to document',exact=True));await mark('回到浅色，接着写','12 / Return to the document')
        elif repo=='satori':
            await click(f.get_by_role('button',name='打开 PDF…',exact=True).first);await f.get_by_title('下一页',exact=True).wait_for();await page.wait_for_timeout(1600);await mark('打开 PDF，直接开始阅读','01 / Open the original sample PDF')
            await click(f.get_by_title('下一页',exact=True));await mark('下一页，阅读位置自然跟随','02 / Page navigation')
            await click(f.get_by_title('放大',exact=True));await click(f.get_by_title('放大',exact=True));await mark('放大页面，看清图文细节','03 / Zoom in')
            await click(f.get_by_title('缩小',exact=True));await click(f.get_by_title('回到适合窗口',exact=True));await mark('一键恢复适合窗口','04 / Zoom out and fit to window')
            await click(f.get_by_role('button',name='双页',exact=True));await mark('双页并排，像翻开一本书','05 / Two-page spread')
            await click(f.get_by_role('button',name='单页',exact=True));await click(f.get_by_title('章节跳转',exact=True));await mark('打开目录，查看章节结构','06 / Open the embedded outline')
            await click(f.get_by_title('跳到第 3 页',exact=True));await mark('从目录直接跳到目标页','07 / Jump to a chapter')
            await click(f.get_by_label('打开问书面板',exact=True));await mark('问书面板：先组织自己的问题','08 / Open the question composer')
            await type_in(f.get_by_label('继续问这本书',exact=True),'请解释这一页的核心观点。');await mark('输入问题；这段演示不伪造 AI 回答','09 / Compose a question, without a fabricated answer')
            await click(f.get_by_title('查看这本书问过的问题',exact=True));await mark('问过的内容，会集中在回看里','10 / Inspect the question-history view')
            await click(f.get_by_label('收起问书面板',exact=True));await mark('收起面板，继续读这一本','11 / Return to reading')
        elif repo=='tick':
            await click(f.get_by_role('button',name='手动填写',exact=True));await type_in(f.get_by_placeholder('每日同步'),'每日笔记整理');await type_in(f.get_by_placeholder('可选备注'),'演示配置，不执行任务');await mark('创建任务，先写清要做什么','01 / Name the task and add a note')
            await click(f.get_by_role('button',name='09:00',exact=True));await mark('每天九点，快速选择时间','02 / Daily schedule preset')
            await click(f.get_by_text('每月某日',exact=True));await mark('每月执行，也可以直接配置','03 / Monthly schedule')
            await click(f.get_by_text('每年某天',exact=True));await mark('一年一次的事，也不用记在脑子里','04 / Yearly schedule')
            await click(f.get_by_text('循环间隔',exact=True));await click(f.get_by_role('button',name='15 分钟',exact=True));await mark('换成每十五分钟循环','05 / Recurring interval presets')
            await click(f.get_by_role('button',name='1 小时',exact=True));code=f.locator('.cm-content[contenteditable=true]').first;await click(code);await page.keyboard.press('Control+A');await page.keyboard.type('// Documentation example only.\nconsole.log("Review the weekly notes");',delay=55);actions.append('Edit inline script');await mark('直接写 Node.js 脚本','06 / Inline script editor')
            await click(f.get_by_text('运行 .js 文件',exact=True));await type_in(f.get_by_placeholder('C:\\Scripts\\daily.js'),'C:\\Scripts\\notes.js');await mark('已有脚本，也可以直接填写路径','07 / Use an existing script file')
            await click(f.get_by_text('高级设置',exact=True));await type_in(f.get_by_placeholder('--env prod "quoted value"'),'--mode preview');await mark('需要时，再展开参数与工作目录','08 / Advanced execution options')
            await click(f.get_by_role('button',name=re.compile('添加变量')));await type_in(f.get_by_placeholder('变量名'),'DEMO_MODE');await type_in(f.get_by_placeholder('变量值'),'preview');await mark('给脚本配置自己的环境变量','09 / Configure non-secret example variables')
            await click(f.get_by_role('button',name=re.compile(r'^取\s*消$')).last);await click(f.get_by_role('button',name='日程',exact=True));await mark('日程视图，集中查看安排','10 / Calendar view; no system task was created')
        else:
            await f.locator('.reader-page img').first.wait_for(state='visible');await page.mouse.move(850,540);await page.mouse.wheel(0,470);await mark('连续阅读，顺着往下看','01 / Continuous reading')
            async def setting(text):
                panel=f.get_by_role('button',name=text,exact=True)
                if not await panel.is_visible():await click(f.get_by_label('阅读设置',exact=True).first)
                await click(panel)
            await setting('宽屏');await page.keyboard.press('Escape');await mark('宽屏阅读，多留一点空间','02 / Wide reading layout')
            await click(f.get_by_label('阅读设置',exact=True).first);await click(f.get_by_role('button',name=re.compile('^贴边')));await page.keyboard.press('Escape');await mark('贴边模式，把页面铺开','03 / Edge-to-edge layout')
            await click(f.get_by_label('阅读设置',exact=True).first);await click(f.get_by_role('button',name=re.compile('^紧凑')));await page.keyboard.press('Escape');await mark('收紧图片间距，阅读更连贯','04 / Compact page spacing')
            await click(f.get_by_label('阅读设置',exact=True).first);await click(f.get_by_role('button',name='单页',exact=True));await page.keyboard.press('Escape');await mark('切成单页，专注眼前这一张','05 / Single-page reading')
            await click(f.get_by_label('下一页',exact=True).first);await mark('用按钮翻到下一页','06 / Page navigation')
            await click(f.get_by_label('阅读设置',exact=True).first);await click(f.get_by_role('button',name='连续',exact=True));await page.keyboard.press('Escape');await click(f.get_by_label('放大阅读页面',exact=True));await click(f.get_by_label('放大阅读页面',exact=True));await mark('放大局部，细节看得更清楚','07 / Zoom into the artwork')
            await click(f.get_by_role('button',name=re.compile('^当前阅读缩放')));await mark('一键恢复原始比例','08 / Reset the reading zoom')
            await click(f.get_by_label('阅读设置',exact=True).first);await click(f.get_by_role('button',name=re.compile('^双页')));await page.keyboard.press('Escape');await mark('双页并排，换一种阅读节奏','09 / Two-page spread')
            await click(f.get_by_label('阅读设置',exact=True).first);await click(f.get_by_role('button',name='适中',exact=True));await click(f.get_by_role('button',name='留白',exact=True));await page.keyboard.press('Escape');await mark('宽度与留白，各自按习惯来','10 / Restore a comfortable reading layout')
        state['success']=True
    except Exception as e:
        state['failure']=str(e);await page.screenshot(path=str(folder/'failure.png'))
        try:state['failure_ui']=await page.frames[1].evaluate("""()=>({text:document.body.innerText,controls:[...document.querySelectorAll('button,input,select,textarea,summary,[role=tab]')].map(e=>({tag:e.tagName,text:e.innerText,title:e.title,aria:e.getAttribute('aria-label'),placeholder:e.getAttribute('placeholder')}))})""")
        except Exception:pass
    state['javascript_errors']=errors
    video=await page.video.path();await context.close()
    if errors:state['success']=False
    if state['success']:
        filters=[]
        for i,scene in enumerate(scenes):
            filters.append(f'[0:v]trim=start={scene["start"]:.4f}:end={scene["end"]:.4f},setpts=(PTS-STARTPTS)/{SPEED},fps=30,tpad=stop_mode=clone:stop_duration={HOLD}[v{i}]')
        filters.append(''.join(f'[v{i}]' for i in range(len(scenes)))+f'concat=n={len(scenes)}:v=1:a=0[v]')
        command=['ffmpeg','-hide_banner','-loglevel','error','-y','-i',str(video),'-filter_complex',';'.join(filters),'-map','[v]','-an','-c:v','libx264','-crf','21','-preset','fast','-pix_fmt','yuv420p','-movflags','+faststart','-threads','2',str(folder/'demo.mp4')]
        result=await asyncio.to_thread(subprocess.run,command,capture_output=True,text=True)
        if result.returncode:state.update(success=False,failure=result.stderr)
        else:
            state['duration']=float(subprocess.check_output(['ffprobe','-v','error','-show_entries','format=duration','-of','csv=p=0',str(folder/'demo.mp4')],text=True))
            state['source_duration']=scenes[-1]['end']-start
            state['sha256']=hashlib.sha256((folder/'demo.mp4').read_bytes()).hexdigest()
            (folder/'poster.png').write_bytes((folder/scenes[-2]['poster']).read_bytes())
    (folder/'provenance.json').write_text(json.dumps(state,ensure_ascii=False,indent=2)+'\n')
    print(repo, 'OK' if state['success'] else state.get('failure',errors),flush=True)
    return state['success']

async def main():
    servers=[]
    for i,repo in enumerate(PROJECTS):
        server=ThreadingHTTPServer(('127.0.0.1',8730+i),functools.partial(Handler,directory=str(ROOT/'dist'/f'demo-frontend-{repo}')))
        threading.Thread(target=server.serve_forever,daemon=True).start();servers.append(server)
    async with async_playwright() as pw:
        browser=await pw.chromium.launch(channel='chrome')
        chosen=os.environ.get('DEMO_PROJECTS',','.join(PROJECTS)).split(',')
        for repo in chosen:
            if repo not in PROJECTS:raise ValueError('Unknown project')
        results=await asyncio.gather(*(record(repo,browser) for repo in chosen));await browser.close()
    for server in servers:server.shutdown()
    if not all(results):raise SystemExit('One or more scenes failed; do not publish failed recordings.')

if __name__=='__main__':asyncio.run(main())
