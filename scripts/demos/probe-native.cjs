// Documentation-only tooling. Never loaded by the application.
const fs = require('node:fs/promises');
const path = require('node:path');
const {chromium} = require(process.env.PLAYWRIGHT_MODULE);
(async () => {
  const out = process.env.DEMO_OUTPUT;
  await fs.mkdir(out, {recursive:true});
  let browser;
  for (let i=0; i<60; i++) {
    try {browser=await chromium.connectOverCDP('http://127.0.0.1:9222', {timeout:1500}); break;}
    catch {await new Promise(r=>setTimeout(r,1000));}
  }
  if (!browser) throw Error('Native WebView2 CDP did not become available');
  await new Promise(r=>setTimeout(r,5000));
  const results=[];
  for (const context of browser.contexts()) {
    for (const page of context.pages()) {
      const i=results.length;
      const data=await page.evaluate(() => ({url:location.href,title:document.title,text:document.body.innerText,
        controls:[...document.querySelectorAll('button,input,textarea,[role="button"],select')].map(e=>({tag:e.tagName,text:e.innerText,placeholder:e.getAttribute('placeholder'),title:e.getAttribute('title'),aria:e.getAttribute('aria-label'),id:e.id,type:e.getAttribute('type')})),
        tauri:!!window.__TAURI_INTERNALS__,size:{width:innerWidth,height:innerHeight}}));
      await page.screenshot({path:path.join(out,`window-${i}.png`)}).catch(e=>{data.screenshotError=e.message});
      results.push(data);
    }
  }
  await fs.writeFile(path.join(out,'windows.json'),JSON.stringify(results,null,2));
  await browser.close();
})().catch(e=>{console.error(e);process.exitCode=1});
