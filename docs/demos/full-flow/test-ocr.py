"""Regression-check real built pointer selection against documentation-only IPC.
No native capture, provider access, personal files or credential are used.
"""
import asyncio, json, os, threading
from http.server import ThreadingHTTPServer
from playwright.async_api import async_playwright
from record import Handler, MEDIA, OUT

async def main():
 server=ThreadingHTTPServer(('127.0.0.1',8791),Handler)
 threading.Thread(target=server.serve_forever,daemon=True).start()
 try:
  async with async_playwright() as pw:
   browser=await pw.chromium.launch(executable_path=os.environ.get('CHROME_BIN','/usr/bin/google-chrome'))
   context=await browser.new_context(viewport={'width':1280,'height':720},device_scale_factor=1.5,locale='zh-CN')
   await context.route('**/*',lambda r:r.continue_() if r.request.url.startswith(('http://127.0.0.1:8791/','blob:http://127.0.0.1:8791/','data:image/')) else r.abort())
   page=await context.new_page();page.set_default_timeout(8000);errors=[];page.on('pageerror',lambda e:errors.append(str(e)))
   async def backend(source,cmd,args):
    assert cmd=='freeze',cmd
    MEDIA['frozen']=(await page.screenshot(),'image/png')
   await context.expose_binding('__backend',backend)
   await page.goto('http://127.0.0.1:8791/desktop.html');await page.frame_locator('#library').get_by_role('button',name='设置',exact=True).wait_for()
   async def calls():return await page.evaluate("state.calls.filter(x=>x.c==='prepare_ocr_request')")
   async def open_mode(name):
    await page.evaluate('launchCapture()');f=page.frame_locator('#overlay');await f.get_by_role('button',name=name,exact=True).click();await f.locator('img').first.evaluate('(im)=>im.decode()');return f
   async def select(start,end):
    await page.mouse.move(*start);await page.mouse.down();await page.mouse.move(*end,steps=24);await page.mouse.up()
   f=await open_mode('文字');await page.mouse.move(194,104);await page.mouse.down();await page.mouse.move(220,120,steps=4);await page.wait_for_timeout(900)
   assert await calls()==[],'OCR fired before pointer release'
   await page.mouse.move(1090,641,steps=32);await page.wait_for_timeout(180)
   assert await calls()==[],'OCR fired while extending the selection'
   await page.mouse.up();await f.get_by_text('识别结果',exact=True).wait_for();await page.wait_for_timeout(200)
   await f.get_by_text('已保存到「文字」',exact=True).wait_for()
   first=await calls();assert len(first)==1,first
   assert first[0]['a']['selection']=={'x':194,'y':104,'width':896,'height':537},first
   await f.get_by_role('button',name='复制',exact=True).click()
   f=await open_mode('文字');await page.mouse.click(120,90);await page.wait_for_timeout(250);assert len(await calls())==1
   await select((1080,580),(200,120));await f.get_by_text('识别结果',exact=True).wait_for()
   second=await calls();assert len(second)==2,second
   assert second[-1]['a']['selection']=={'x':200,'y':120,'width':880,'height':460}
   await f.get_by_role('button',name='复制',exact=True).click()
   f=await open_mode('截图');await select((210,150),(1015,530));assert len(await calls())==2
   await f.get_by_role('button',name='文字',exact=True).click();await f.get_by_text('识别结果',exact=True).wait_for();await page.wait_for_timeout(400)
   third=await calls();assert len(third)==3,third
   assert third[-1]['a']['selection']=={'x':210,'y':150,'width':805,'height':380}
   assert not errors,errors
   await page.screenshot(path=str(OUT/'ocr-selection-check.png'))
   (OUT/'ocr-selection-check.json').write_text(json.dumps({'passed':['no partial-drag request','one request on release','full release bounds','click does not recognize','reverse drag','reuse existing selection once'],'requests':[x['a']['selection'] for x in third]},ensure_ascii=False,indent=2)+'\n')
   await context.close();await browser.close()
 finally:
  server.shutdown();server.server_close()
if __name__=='__main__':asyncio.run(main())
