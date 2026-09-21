// Run after `dx build --web` in graphtron-demo. Uses the same Playwright install
// as crates/graphtron/tests/browser.cjs; no platform services are needed.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const http = require('node:http');
const { chromium } = require('playwright');
const repo = path.resolve(__dirname, '../../..');
const root = path.join(repo, 'target/dx/graphtron-demo/debug/web/public');
const out = path.join(repo, 'target/graphtron-browser');
assert(fs.existsSync(path.join(root, 'index.html')), 'Build graphtron-demo first.');
const server = http.createServer((req,res)=>{
  const url = new URL(req.url,'http://localhost');
  const file = path.resolve(root, '.' + (url.pathname==='/'?'/index.html':decodeURIComponent(url.pathname)));
  if (!file.startsWith(root+path.sep) || !fs.existsSync(file) || !fs.statSync(file).isFile()) {
    res.writeHead(404);res.end();return;
  }
  const mime={'.html':'text/html','.js':'text/javascript','.wasm':'application/wasm','.css':'text/css'};
  res.setHeader('Content-Type',mime[path.extname(file)]||'application/octet-stream');
  fs.createReadStream(file).pipe(res);
});
(async()=>{
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  const browser=await chromium.launch({headless:true});
  const errors=[];
  try {
    const page=await browser.newPage({viewport:{width:1100,height:900}});
    page.setDefaultTimeout(15000);
    page.on('pageerror',error=>errors.push(error.message));
    await page.route('**/*',route=>new URL(route.request().url()).hostname==='127.0.0.1'?route.continue():route.abort());
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.getByRole('heading',{name:'Dense data inspection',exact:true}).waitFor();
    await page.evaluate(()=>{
      window.hoverLabels=[];
      const fill=CanvasRenderingContext2D.prototype.fillText;
      CanvasRenderingContext2D.prototype.fillText=function(value,...args){
        window.hoverLabels.push(String(value));
        return fill.call(this,value,...args);
      };
    });
    for(const [title, expected] of [['Pie Chart','Service'],['Treemap Widget','Service'],['Host Map Widget','host-']]) {
      const widget=page.locator('.card').filter({has:page.locator('.title',{hasText:title})});
      await widget.scrollIntoViewIfNeeded();
      const canvas=widget.locator('canvas').nth(1);
      const box=await canvas.boundingBox();
      await page.evaluate(()=>window.hoverLabels=[]);
      await page.mouse.move(box.x+box.width*.3,box.y+box.height*.3);
      await page.waitForFunction(expected=>window.hoverLabels.some(text=>text.includes(expected)),expected);
      fs.mkdirSync(out,{recursive:true});
      await widget.screenshot({path:path.join(out,`${title.replaceAll(' ','-').toLowerCase()}.png`)});
    }
    for(const title of ['Point Plot','Scatter Plot Widget']) {
      assert.equal(await page.locator('.card').filter({has:page.locator('.title',{hasText:title})}).locator('canvas').count(),2);
    }
    const controls=page.locator('.density-controls');
    await controls.locator('select').nth(0).selectOption('1000000');
    await controls.getByRole('button',{name:'Inspect 101 samples',exact:true}).click();
    for(const kind of ['area','band','scatter','line']) {
      await controls.locator('select').nth(1).selectOption(kind);
      await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
    }
    await controls.getByRole('button',{name:'Show all',exact:true}).click();
    const card=page.locator('.inspection-card');
    await card.scrollIntoViewIfNeeded();
    const overlay=card.locator('canvas').nth(1);
    await overlay.evaluate(el=>{
      window.graphtronEvents=[];
      for(const type of ['pointerdown','pointerup','pointerleave','pointercancel'])el.addEventListener(type,event=>window.graphtronEvents.push({type,x:event.offsetX,y:event.offsetY,primary:event.isPrimary}));
    });
    const bounds=await overlay.boundingBox();
    await overlay.click({position:{x:bounds.width*.5,y:bounds.height*.5}});
    const inspector=card.locator('.graphtron-tooltip-inspector');
    await inspector.waitFor().catch(async error=>{
      fs.mkdirSync(out,{recursive:true});
      await page.screenshot({path:path.join(out,'gallery-failure.png'),fullPage:true});
      console.error('Browser errors:',errors,await page.evaluate(()=>window.graphtronEvents));
      throw error;
    });
    await inspector.getByRole('button',{name:'Inspect values',exact:true}).click();
    assert(await page.evaluate(()=>window.graphtronEvents.some(event=>event.type==='pointerup')),'Pointer capture lost pointerup');
    const firstPage=await inspector.innerText();
    await inspector.getByRole('button',{name:'Next',exact:true}).click();
    assert.notEqual(await inspector.innerText(),firstPage,'Next did not expose omitted series');
    await inspector.getByRole('button',{name:'Next',exact:true}).click();
    assert(await inspector.getByRole('button',{name:'Next',exact:true}).isDisabled(),'Last page is not bounded');
    await page.mouse.move(10,10);
    assert(await inspector.isVisible(),'Frozen details disappeared when pointer left');
    await card.locator('[role=region]').focus();
    await page.keyboard.press('Escape');
    await inspector.waitFor({state:'hidden'});
    await page.setViewportSize({width:390,height:844});
    await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
    assert(await page.evaluate(()=>document.documentElement.scrollWidth<=window.innerWidth+1),'Gallery overflows mobile width');
    fs.mkdirSync(out,{recursive:true});
    await page.screenshot({path:path.join(out,'gallery-mobile.png'),fullPage:true});
    assert.deepEqual(errors,[],'Gallery emitted browser exceptions');
    console.log('Graphtron gallery passed: new widget hover, million-point type switching, frozen pagination, Escape, mobile layout.');
  } finally { await browser.close();server.close(); }
})().catch(error=>{console.error(error);server.close();process.exitCode=1;});
