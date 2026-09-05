"""Inspect unmodified production frontends in an isolated documentation sandbox."""
import asyncio
import functools
import json
import threading
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from playwright.async_api import async_playwright

ROOT = Path(__file__).resolve().parent
PROJECTS = ['kiri','mimi','satori','viva','tick','wnacg']
OUT = ROOT/'inspection'
OUT.mkdir(exist_ok=True)

class Handler(SimpleHTTPRequestHandler):
    def log_message(self, *args): pass
    def translate_path(self, path):
        if path.startswith('/__demo__/'):
            return str(ROOT/'fixtures'/path.split('?')[0].rsplit('/',1)[-1])
        return super().translate_path(path)

async def inspect(repo, browser):
    port=8730+PROJECTS.index(repo)
    ctx=await browser.new_context(viewport={'width':1280,'height':800},locale='en-US')
    async def route(request):
        url=request.request.url
        if url.startswith(f'http://127.0.0.1:{port}/'):
            await request.continue_()
        elif url.startswith('https://img.qy0.ru/demo/page-') and url.endswith('.png'):
            name=url.rsplit('/',1)[-1]
            if name not in ['page-1.png','page-2.png','page-3.png']:
                await request.abort();return
            await request.fulfill(path=str(ROOT/'fixtures'/name),content_type='image/png')
        else:
            await request.abort()
    await ctx.route('**/*',route)
    await ctx.add_init_script(script=(ROOT/'bridge.js').read_text().replace('__PROJECT__',repo))
    page=await ctx.new_page()
    errors=[]
    page.on('pageerror',lambda error:errors.append(str(error)))
    q='?window=editor&id=demo' if repo=='kiri' else ''
    try:
        await page.goto(f'http://127.0.0.1:{port}/{q}')
        await page.wait_for_timeout(1800)
        if repo=='viva':
            await page.get_by_role('button',name='Open folder',exact=True).click()
            await page.get_by_text('Weekend.md',exact=True).first.click()
        elif repo=='satori':
            await page.get_by_role('button',name='打开 PDF…',exact=True).first.click()
        elif repo=='tick':
            await page.get_by_role('button',name='手动新建',exact=True).first.click()
        elif repo=='wnacg':
            await page.get_by_text('Small moments — original sample',exact=True).first.click()
        await page.wait_for_timeout(2000)
    except Exception as error:
        errors.append(str(error))
    await page.screenshot(path=str(OUT/f'{repo}.png'))
    state=await page.evaluate("""() => ({text:document.body.innerText,controls:[...document.querySelectorAll('button,input,textarea,select,[contenteditable=true]')].map(e=>({tag:e.tagName,text:e.innerText,placeholder:e.getAttribute('placeholder'),title:e.getAttribute('title'),aria:e.getAttribute('aria-label'),id:e.id,type:e.getAttribute('type')})),calls:window.__demoCalls})""")
    state['errors']=errors
    (OUT/f'{repo}.json').write_text(json.dumps(state,ensure_ascii=False,indent=2),encoding='utf-8')
    print(repo, state['text'][:300], errors[:2])
    await ctx.close()

async def main():
    servers=[]
    for i,repo in enumerate(PROJECTS):
        directory=ROOT/'dist'/f'demo-frontend-{repo}'
        server=ThreadingHTTPServer(('127.0.0.1',8730+i),functools.partial(Handler,directory=str(directory)))
        threading.Thread(target=server.serve_forever,daemon=True).start()
        servers.append(server)
    async with async_playwright() as pw:
        browser=await pw.chromium.launch()
        await asyncio.gather(*(inspect(repo,browser) for repo in PROJECTS))
        await browser.close()
    for server in servers:server.shutdown()

if __name__=='__main__':asyncio.run(main())
