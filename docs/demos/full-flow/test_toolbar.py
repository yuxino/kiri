"""Built overlay layout regression with documentation-only IPC, never native capture."""
import asyncio
import json
import os
import struct
import threading
from http.server import ThreadingHTTPServer
from pathlib import Path
from playwright.async_api import async_playwright
from record import Handler, MEDIA, OUT

ROOT = Path(__file__).resolve().parents[3]

async def main():
    server = ThreadingHTTPServer(('127.0.0.1', 8791), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    report = {'native': False, 'checks': []}
    try:
        async with async_playwright() as pw:
            browser = await pw.chromium.launch(executable_path=os.environ.get('CHROME_BIN', '/usr/bin/google-chrome'))
            try:
                # Exercise backing scale independently from each logical viewport.
                for width, height, scale in [(w, h, s) for w,h in [(800,600), (640,480), (1512,982)] for s in [1,1.25,1.5,2]]:
                    for language in ['en', 'zh-Hans', 'ja']:
                        translations = json.loads((ROOT / 'src/i18n' / (language + '.json')).read_text())
                        context = await browser.new_context(viewport={'width': width, 'height': height}, device_scale_factor=scale, locale=language)
                        try:
                            await context.route('**/*', lambda r: r.continue_() if r.request.url.startswith(('http://127.0.0.1:8791/', 'blob:http://127.0.0.1:8791/', 'data:image/')) else r.abort())
                            page = await context.new_page()
                            page.set_default_timeout(8000)
                            errors = []
                            page.on('pageerror', lambda e: errors.append(str(e)))
                            async def backend(source, command, args):
                                if command == 'freeze':
                                    MEDIA['frozen'] = (await page.screenshot(), 'image/png')
                                else:
                                    assert command == 'save_png', command
                                    data = bytes(args['bytes'])
                                    assert data[:8].hex() == '89504e470d0a1a0a'
                                    assert struct.unpack('>II', data[16:24]) == (round(140*scale), round(160*scale))
                            await context.expose_binding('__backend', backend)
                            await page.goto('http://127.0.0.1:8791/desktop.html')
                            await page.wait_for_function('typeof window.invoke === "function"')
                            await page.evaluate('''({width,height,scale,language}) => {
                                const original = window.invoke;
                                window.invoke = (kind, command, args) => {
                                    if (command === 'get_language' || command === 'get_locale') return Promise.resolve(language);
                                    if (command === 'start_capture') return Promise.resolve({displayWidth:width,displayHeight:height,scale,pixelWidth:width*scale,pixelHeight:height*scale,windowRects:[],sourceApplication:'Public fixture'});
                                    return original(kind, command, args);
                                };
                            }''', {'width': width, 'height': height, 'scale': scale, 'language': language})
                            async def check(f):
                                done = f.get_by_role('button', name=translations['Done — Copy to clipboard · Return'], exact=True)
                                await done.wait_for()
                                toolbar = f.locator('.kiri-capture-toolbar')
                                # Wait for ResizeObserver placement to settle after tool reflow.
                                await page.wait_for_timeout(100)
                                geometry = await toolbar.evaluate('''el => {
                                    const rect = e => {const r=e.getBoundingClientRect(); return {x:r.x,y:r.y,width:r.width,height:r.height};};
                                    return {bar:rect(el), controls:[...el.querySelectorAll('button,input')].map(rect)};
                                }''')
                                for r in [geometry['bar'], *geometry['controls']]:
                                    assert r['x'] >= 7.5 and r['y'] >= 95.5, geometry
                                    assert r['x'] + r['width'] <= width - 7.5, geometry
                                    assert r['y'] + r['height'] <= height - 7.5, geometry
                                return done, geometry
                            # Four corners at 100%; each remaining scale repeats the exact edge regression.
                            corners = [(10,10), (width-150,10), (10,height-180), (width-150,height-180)] if scale == 1 else [(width-150,height-180)]
                            for x, y in corners:
                                await page.evaluate('launchCapture()')
                                f = page.frame_locator('#overlay')
                                await f.locator('img').first.evaluate('(im)=>im.decode()')
                                await page.mouse.move(x, y)
                                await page.mouse.down()
                                await page.mouse.move(x+140, y+160, steps=10)
                                await page.mouse.up()
                                done, geometry = await check(f)
                                # The original #65 selection is 650,420 -> 790,580.
                                if x == width-150 and y == height-180:
                                    assert geometry['bar']['y'] + geometry['bar']['height'] <= y, geometry
                                    if width == 800 and language == 'en' and scale == 1:
                                        await page.screenshot(path=str(OUT / 'toolbar-800x600-renderer-fixture.png'))
                                    more = f.get_by_role('button', name=translations['More Actions'], exact=True)
                                    assert await more.get_attribute('aria-expanded') == 'false'
                                    await more.focus()
                                    await more.press('Enter')
                                    _, expanded = await check(f)
                                    assert expanded['bar']['y'] + expanded['bar']['height'] <= y
                                    assert await more.get_attribute('aria-expanded') == 'true'
                                    await f.get_by_title(translations['Width (px)'], exact=True).fill(str(round(140*scale)))
                                    await f.get_by_title(translations['Height (px)'], exact=True).fill(str(round(160*scale)))
                                    # Return applies size rather than finishing capture.
                                    await f.get_by_title(translations['Height (px)'], exact=True).press('Enter')
                                    await done.wait_for()
                                    await more.focus()
                                    await more.press('Space')
                                    _, collapsed = await check(f)
                                    assert collapsed['bar']['height'] < expanded['bar']['height']
                                    assert await more.get_attribute('aria-expanded') == 'false'
                                    for tool in ['Text (T)', 'Mosaic (M)', 'Pen (P)', 'Select (V)']:
                                        await f.get_by_role('button', name=translations[tool], exact=True).click()
                                        done, geometry = await check(f)
                                        assert geometry['bar']['y'] + geometry['bar']['height'] <= y
                                        assert await more.get_attribute('aria-expanded') == ('false' if tool == 'Select (V)' else 'true')
                                        assert await done.get_attribute('title') == translations['Done — Copy to clipboard · Return']
                                    await done.click()
                                    await page.wait_for_function('state.calls.some(x=>x.c==="confirm_capture")')
                                    request = await page.evaluate('state.pendingAnnotation.selection')
                                    assert request == {'x': x, 'y': y, 'width': 140, 'height': 160}, request
                                else:
                                    await f.get_by_role('button', name=translations['Cancel capture · Esc'], exact=True).click()
                                await page.locator('#overlay').wait_for(state='detached')
                                report['checks'].append({'viewport': [width,height], 'scale': scale, 'language': language, 'selection_origin': [x,y]})
                            assert not errors, errors
                        finally:
                            await context.close()
            finally:
                await browser.close()
        (OUT / 'toolbar-check.json').write_text(json.dumps(report, indent=2)+'\n')
        print('PASS: 63 corner/scale selections, 36 disclosure/size/tool-reflow and pointer-completion cases; renderer fixture only')
    finally:
        server.shutdown()
        server.server_close()

if __name__ == '__main__':
    asyncio.run(main())
