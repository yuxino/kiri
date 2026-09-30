"""Actual built image UI against isolated IPC; no native capture or user files."""
import asyncio, io, json, os, sys, threading
from http.server import ThreadingHTTPServer
from PIL import Image
from playwright.async_api import async_playwright
from record import Handler, MEDIA, OUT

async def main():
 server=ThreadingHTTPServer(('127.0.0.1',8791),Handler)
 threading.Thread(target=server.serve_forever,daemon=True).start()
 try:
  async with async_playwright() as pw:
   browser=await pw.chromium.launch(executable_path=os.environ.get('CHROME_BIN','/usr/bin/google-chrome'))
   try:
    context=await browser.new_context(viewport={'width':1280,'height':720},device_scale_factor=1,locale='en-US')
    await context.route('**/*',lambda r:r.continue_() if r.request.url.startswith(('http://127.0.0.1:8791/','blob:http://127.0.0.1:8791/','data:image/')) else r.abort())
    page=await context.new_page();page.set_default_timeout(8000)
    errors=[];page.on('pageerror',lambda e:errors.append(str(e)))
    async def backend(source,cmd,args):
     if cmd=='freeze':MEDIA['frozen']=(await page.screenshot(),'image/png')
     elif cmd=='save_png':MEDIA[args['id']]=(bytes(args['bytes']),'image/png')
     else:raise AssertionError(cmd)
    await context.expose_binding('__backend',backend)
    await page.goto('http://127.0.0.1:8791/desktop.html')
    await page.wait_for_function('typeof window.invoke==="function"')
    await page.evaluate("""() => {
     const original=window.invoke;state.updates=0;state.failSave=false;
     window.invoke=async(kind,c,a)=>{
      if(c==='get_language'||c==='get_locale')return 'en';
      if(c==='start_capture')return {displayWidth:1280,displayHeight:720,scale:1,pixelWidth:1280,pixelHeight:720,windowRects:[]};
      if(c==='prepare_asset_annotation'){state.prepared=JSON.parse(a.documentJson);return 'fixture-edit';}
      if(c==='update_asset'){state.updates++;if(state.failSave)throw Error('isolated save failure');return {revisionSha256:'b'.repeat(64),actionSucceeded:true};}
      if(c==='save_file_dialog')return state.cancelSaveAs?null:'fixture-save';
      return original(kind,c,a);
     };
    }""")
    mod='Meta' if sys.platform=='darwin' else 'Control'
    async def drag(start,end):
     await page.mouse.move(*start);await page.mouse.down();await page.mouse.move(*end,steps=8);await page.mouse.up()
    async def capture():
     await page.evaluate('launchCapture()');f=page.frame_locator('#overlay')
     await f.locator('img').first.evaluate('(im)=>im.decode()')
     await drag((180,140),(1050,570));return f
    f=await capture()
    await f.get_by_title('Rectangle (R)',exact=True).click();await drag((230,180),(500,250))
    await f.get_by_title('Text (T)',exact=True).click();await page.mouse.click(620,310)
    textarea=f.get_by_role('textbox',name='Text content',exact=True)
    await textarea.press_sequentially('test123',delay=40)
    await textarea.press(mod+'+z')
    before=os.environ.get('KIRI_IMAGE_EDIT_BEFORE')=='1'
    if before:
     assert await textarea.count()==0,'baseline no longer reproduces canvas undo'
     await page.screenshot(path=str(OUT/'text-undo-before.png'))
     await page.keyboard.press(mod+'+Shift+z');await page.mouse.click(650,330)
     await textarea.wait_for();await textarea.press('Escape')
     await page.locator('#overlay').wait_for(state='detached')
     await page.screenshot(path=str(OUT/'text-escape-before.png'))
     buffer=io.BytesIO();Image.new('RGB',(1000,600),'#707b8d').save(buffer,format='PNG');MEDIA['image-edit-fixture']=(buffer.getvalue(),'image/png')
     await page.evaluate("frame('editor',{id:'image-edit-fixture'})")
     editor=page.frame_locator('#editor');await editor.locator('canvas').wait_for()
     await editor.get_by_title('Text (T)',exact=True).click();await page.mouse.click(600,320)
     await editor.get_by_role('textbox',name='Text content',exact=True).press_sequentially('unsaved text',delay=20)
     await editor.get_by_role('button',name='Cancel',exact=True).click();await page.locator('#editor').wait_for(state='detached')
     await page.screenshot(path=str(OUT/'image-close-before.png'))
     (OUT/'image-editing-before.json').write_text(json.dumps({'native':False,'reproduced':['text undo exits input and removes whole annotation','first Escape closes capture','dirty image closes without prompt']},indent=2)+'\n')
     assert not errors,errors
     await context.close();return
    assert await textarea.count()==1,'text undo removed the annotation editor'
    assert await textarea.input_value()!='test123','native text undo did not change input'
    await page.screenshot(path=str(OUT/'text-undo-after.png'))
    await textarea.press(mod+'+Shift+z');assert await textarea.input_value()=='test123'
    await textarea.press('Shift+Enter');await textarea.press_sequentially('second line',delay=10)
    assert '\n' in await textarea.input_value()
    await f.get_by_text('Shift + Enter: new line · Enter: done · Esc: cancel edit',exact=True).wait_for()
    # Composition events are injected only to check routing, not native IME acceptance.
    await textarea.evaluate("el=>el.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',isComposing:true,bubbles:true}))")
    await textarea.evaluate("el=>el.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',isComposing:true,bubbles:true}))")
    assert await textarea.count()==1
    await textarea.press('Escape');await textarea.wait_for(state='detached')
    assert await page.locator('#overlay').count()==1,'first Escape closed capture'
    await page.screenshot(path=str(OUT/'text-escape-after.png'))
    await f.get_by_title('Undo (⌘Z)',exact=True).click()
    assert await f.get_by_title('Redo (⇧⌘Z)',exact=True).is_enabled(),'earlier rectangle was lost'
    await f.get_by_title('Redo (⇧⌘Z)',exact=True).click()
    await page.keyboard.press('Escape');await page.locator('#overlay').wait_for(state='detached')
    f=await capture();await f.get_by_title('Text (T)',exact=True).click();await page.mouse.click(620,310)
    textarea=f.get_by_role('textbox',name='Text content',exact=True)
    await textarea.press_sequentially('line one',delay=10);await textarea.press('Shift+Enter');await textarea.press_sequentially('line two',delay=10)
    await textarea.press('Enter');await page.locator('#overlay').wait_for(state='detached')
    assert (await page.evaluate('state.pendingAnnotation'))['documentJson'].find('line one')!=-1
    # Seed one public image for saved-image editor tests.
    buffer=io.BytesIO();Image.new('RGB',(1000,600),'#707b8d').save(buffer,format='PNG');MEDIA['image-edit-fixture']=(buffer.getvalue(),'image/png')
    async def editor():
     await page.evaluate("frame('editor',{id:'image-edit-fixture'})")
     f=page.frame_locator('#editor');await f.locator('canvas').wait_for();await page.wait_for_timeout(120);return f
    f=await editor();await f.get_by_role('button',name='Cancel',exact=True).click()
    await page.locator('#editor').wait_for(state='detached')
    f=await editor();await f.get_by_title('Text (T)',exact=True).click();await page.mouse.click(600,320)
    textarea=f.get_by_role('textbox',name='Text content',exact=True)
    await textarea.press_sequentially('unsaved text',delay=20)
    await f.get_by_role('button',name='Cancel',exact=True).click()
    dialog=f.get_by_role('dialog',name='Save changes before closing?',exact=True);await dialog.wait_for()
    await page.screenshot(path=str(OUT/'image-close-after.png'))
    await dialog.get_by_role('button',name='Keep editing',exact=True).click();await dialog.wait_for(state='hidden')
    assert await textarea.input_value()=='unsaved text'
    await textarea.press(mod+'+w');await dialog.wait_for()
    await dialog.get_by_role('button',name='Keep editing',exact=True).click();await dialog.wait_for(state='hidden')
    await textarea.press('Escape');await textarea.wait_for(state='detached')
    # The discarded pending text leaves a clean editor, so ordinary close works.
    await f.get_by_role('button',name='Cancel',exact=True).click();await page.locator('#editor').wait_for(state='detached')
    f=await editor();await f.get_by_title('Rectangle (R)',exact=True).click();await drag((300,220),(650,370))
    # Native close-request event is IPC-injected; the rendered modal must prevent closure.
    await page.locator('#editor').evaluate("el=>el.contentWindow.__emit('tauri://close-requested',null)")
    dialog=f.get_by_role('dialog',name='Save changes before closing?',exact=True);await dialog.wait_for()
    await dialog.get_by_role('button',name='Keep editing',exact=True).click()
    await f.get_by_title('Undo (⌘Z)',exact=True).click();await f.get_by_role('button',name='Cancel',exact=True).click()
    await page.locator('#editor').wait_for(state='detached')
    f=await editor();await f.get_by_title('Rectangle (R)',exact=True).click();await drag((300,220),(650,370))
    await page.evaluate('state.cancelSaveAs=true');await f.get_by_role('button',name='Save As…',exact=True).click()
    assert await page.evaluate('state.updates')==0
    await f.get_by_role('button',name='Cancel',exact=True).click();dialog=f.get_by_role('dialog',name='Save changes before closing?',exact=True);await dialog.wait_for()
    await page.evaluate('state.failSave=true')
    await dialog.get_by_role('button',name='Save & close',exact=True).click()
    await dialog.get_by_role('alert').wait_for();assert await page.locator('#editor').count()==1
    await page.evaluate('state.failSave=false')
    await dialog.get_by_role('button',name='Save & close',exact=True).click();await page.locator('#editor').wait_for(state='detached')
    assert len((await page.evaluate('state.prepared'))['marks'])==1
    f=await editor();await f.get_by_title('Rectangle (R)',exact=True).click();await drag((300,220),(650,370))
    await f.get_by_role('button',name='Cancel',exact=True).click();dialog=f.get_by_role('dialog',name='Save changes before closing?',exact=True)
    await dialog.get_by_role('button',name='Discard unsaved changes & close',exact=True).click()
    await page.locator('#editor').wait_for(state='detached')
    # Successful Save As updates the close-warning baseline.
    f=await editor();await f.get_by_title('Rectangle (R)',exact=True).click();await drag((300,220),(650,370))
    await page.evaluate('state.cancelSaveAs=false');await f.get_by_role('button',name='Save As…',exact=True).click()
    await page.wait_for_function('state.updates===3');await page.wait_for_timeout(100)
    await f.get_by_role('button',name='Cancel',exact=True).click();await page.locator('#editor').wait_for(state='detached')
    assert not errors,errors
    (OUT/'image-editing-check.json').write_text(json.dumps({'native':False,'passed':['native textarea undo/redo routing','Shift+Enter multiline','localized hint','composition event routing only','first Escape retains capture and marks','second Escape cancels','Enter commits text and completes capture','clean editor close','Cmd/Ctrl+W from focused text','dirty text keep editing','cancel text restores clean baseline','native close event guard','undo to baseline','Save As cancel is no-op','failed save retains dialog and marks','save then close','discard then close','Save As baseline']},indent=2)+'\n')
    await context.close()
   finally:await browser.close()
 finally:server.shutdown();server.server_close()

if __name__=='__main__':asyncio.run(main())
