// Canvas integration regressions. Build examples/browser_fixture.rs and run
// wasm-bindgen into target/graphtron-browser before invoking this script.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const http = require('node:http');
const { chromium } = require('playwright');
const repo = path.resolve(__dirname, '../../..');
const out = path.join(repo, 'target/graphtron-browser');
const results = [];
const failures = [];
fs.mkdirSync(out, { recursive: true });
assert(fs.existsSync(path.join(out, 'browser_fixture_bg.wasm')), 'Build the WASM fixture first; see graphtron README.');
const html = `<html><body style="margin:8px;background:#ddd;font:14px monospace"><script type="module">
import init,* as fixture from '/browser_fixture.js'; await init(); window.fixture=fixture; window.ready=true;
</script></body></html>`;
const server = http.createServer((req, res) => {
  const name = new URL(req.url, 'http://localhost').pathname;
  if (name === '/') { res.setHeader('Content-Type', 'text/html'); res.end(html); return; }
  if (!['/browser_fixture.js', '/browser_fixture_bg.wasm'].includes(name)) { res.writeHead(404); res.end(); return; }
  res.setHeader('Content-Type', name.endsWith('.wasm') ? 'application/wasm' : 'text/javascript');
  fs.createReadStream(path.join(out, name.slice(1))).pipe(res);
});
function check(name, fn) {
  try { fn(); } catch (error) { failures.push(`${name}: ${error.message}`); }
}
function white(pixel) { return pixel.slice(0, 3).every(v => v >= 250); }
async function render(page, name, w = 400, h = 180, n = 0) {
  console.log(`Render ${name} ${w}x${h} n=${n}`);
  const rendering = page.evaluate(({name,w,h,n}) => {
    const label = document.createElement('div'); label.textContent = `${name} (${w}×${h}, n=${n})`; document.body.append(label);
    const canvas = document.createElement('canvas'); canvas.style.width = `${w}px`; canvas.style.height = `${h}px`; document.body.append(canvas);
    const proto = CanvasRenderingContext2D.prototype;
    const saved = {}, counts = {}, text = [], invalid = [], strokes = [];
    let currentPath = [];
    for (const method of ['moveTo','lineTo','bezierCurveTo','arc','fillRect','strokeRect']) {
      saved[method] = proto[method];
      proto[method] = function(...args) {
        counts[method] = (counts[method] || 0) + 1;
        if (method==='moveTo' || method==='lineTo') currentPath.push({method,args});
        if (args.some(arg => typeof arg === 'number' && !Number.isFinite(arg))) invalid.push({method,args});
        return saved[method].apply(this,args);
      };
    }
    saved.beginPath = proto.beginPath;
    proto.beginPath = function(...args) { currentPath=[]; return saved.beginPath.apply(this,args); };
    saved.stroke = proto.stroke;
    proto.stroke = function(...args) { strokes.push({width:this.lineWidth,color:this.strokeStyle,path:currentPath.slice()}); return saved.stroke.apply(this,args); };
    saved.fillText = proto.fillText;
    proto.fillText = function(value,x,y,...args) {
      const metrics = this.measureText(value);
      // Record logical text bounds, including rotation, in CSS pixels.
      const m = this.getTransform(); const dpr = window.devicePixelRatio;
      const x0 = x-metrics.actualBoundingBoxLeft, x1=x+metrics.actualBoundingBoxRight;
      const y0 = y-metrics.actualBoundingBoxAscent, y1=y+metrics.actualBoundingBoxDescent;
      const corners = [[x0,y0],[x1,y0],[x0,y1],[x1,y1]].map(([x,y]) => [(m.a*x+m.c*y+m.e)/dpr,(m.b*x+m.d*y+m.f)/dpr]);
      text.push({value,x,y,left:Math.min(...corners.map(p=>p[0])),right:Math.max(...corners.map(p=>p[0])),top:Math.min(...corners.map(p=>p[1])),bottom:Math.max(...corners.map(p=>p[1]))});
      return saved.fillText.call(this,value,x,y,...args);
    };
    let layout=[],error;
    try {
      if (name==='tooltip') window.fixture.tooltip(canvas,w,h,n);
      else if (name.endsWith('_hover')) layout=Array.from(window.fixture.hover_fixture(canvas,name));
      else layout=Array.from(window.fixture.render(canvas,name,w,h,n));
    } catch (e) { error=String(e); }
    finally { for (const method in saved) proto[method]=saved[method]; }
    const ctx=canvas.getContext('2d'), dpr=window.devicePixelRatio;
    const pixel=(x,y)=>Array.from(ctx.getImageData(Math.max(0,Math.min(canvas.width-1,Math.floor(x*dpr))),Math.max(0,Math.min(canvas.height-1,Math.floor(y*dpr))),1,1).data);
    const [x=1,y=1,pw=w-2,ph=h-2]=layout;
    return {name,w,h,n,dpr,error,layout,counts,invalid,text,strokes,pixels:{border:pixel(x,y+ph*.25),top:pixel(x+pw*.5,y+ph*.1),middle:pixel(x+pw*.5,y+ph*.5),low:pixel(x+pw*.5,y+ph*.9),row75:pixel(x+pw*.5,y+ph*.75)}};
  },{name,w,h,n});
  let timer;
  try {
    return await Promise.race([
      rendering,
      new Promise((_,reject)=>{timer=setTimeout(()=>reject(new Error(`${name} exceeded 30 seconds`)),30000);}),
    ]);
  } finally { clearTimeout(timer); }
}
(async () => {
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  const browser=await chromium.launch({headless:true});
  try {
    for (const dpr of [1,1.25,1.5,2,3]) {
      const page=await browser.newPage({viewport:{width:850,height:1000},deviceScaleFactor:dpr});
      await page.goto(`http://127.0.0.1:${server.address().port}`);
      await page.waitForFunction(()=>window.ready);
      const cases=[['grid_only'],['line'],['area'],['area_markers'],['heat',400,100,200],['axis'],['logaxis',400,80],['logstep'],['band_short'],['hist_ragged'],['tiny_labels',80,8],['decorations',280,180,40],['hist'],['hbar'],['ohlc'],['state'],['band'],['tooltip',240,160,10]];
      for (const args of cases) {
        const r=await render(page,...args); results.push(r);
        const name=`${r.name}@${dpr}`;
        check(name,()=>assert(!r.error,r.error));
        check(`${name} finite drawing`,()=>assert.equal(r.invalid.length,0,JSON.stringify(r.invalid.slice(0,3))));
        if (r.name==='grid_only') check(`${name} strokes align with device pixels`,()=>{
          assert(r.strokes.length>0,'missing grid strokes');
          for (const stroke of r.strokes) {
            const path=stroke.path;
            if(path.length!==2)continue;
            const a=path[0].args,b=path[1].args;
            const coordinate=a[0]===b[0]?a[0]:a[1]===b[1]?a[1]:null;
            if(coordinate===null)continue;
            for(const sign of [-1,1]) {
              const edge=(coordinate+sign*stroke.width/2)*dpr;
              assert(Math.abs(edge-Math.round(edge))<1e-6,`edge ${edge}, ${JSON.stringify(stroke)}`);
            }
          }
        });
        if (r.name==='line') check(`${name} no frame`,()=>assert(white(r.pixels.border),JSON.stringify(r.pixels.border)));
        if (r.name.startsWith('area')) {
          check(`${name} no fill above data`,()=>assert(white(r.pixels.top),JSON.stringify(r.pixels.top)));
          check(`${name} fill below data`,()=>assert(!white(r.pixels.low),'area did not fill'));
        }
        if (r.name==='logstep') check(`${name} missing interval`,()=>assert(white(r.pixels.middle),JSON.stringify(r.pixels.middle)));
        if (r.name==='heat') check(`${name} row agrees with hover`,()=>{
          assert.equal(r.layout[7],10); assert(r.pixels.row75[0]>200 && r.pixels.row75[1]>180 && r.pixels.row75[2]<100,JSON.stringify(r.pixels.row75));
        });
        if (['axis','logaxis','tooltip','decorations'].includes(r.name)) {
          check(`${name} text fits canvas`,()=>assert(r.text.every(t=>t.left>=-1 && t.right<=r.w+1 && t.top>=-1 && t.bottom<=r.h+1),JSON.stringify(r.text.filter(t=>t.left< -1 || t.right>r.w+1 || t.top< -1 || t.bottom>r.h+1))));
        }
        if (r.name==='logaxis') check(`${name} ticks do not collide`,()=>{
          const ticks=r.text.filter(t=>/^\d+$/.test(t.value)).sort((a,b)=>a.top-b.top);
          for(let i=1;i<ticks.length;i++)assert(ticks[i].top>=ticks[i-1].bottom+1,JSON.stringify(ticks));
        });
        if (r.name==='tooltip') check(`${name} value stays visible`,()=>assert(r.text.some(t=>t.value==='2' && t.left>=0 && t.right<=r.w),'missing numeric value column'));
      }
      for (const kind of ['pie','treemap','hosts','point','scatter_cloud','delta_hover','ohlc_hover','state_hover','pie_hover']) {
        const r=await render(page,kind); results.push(r);
        check(`${kind}@${dpr}`,()=>{
          assert(!r.error,r.error);
          assert.equal(r.invalid.length,0);
          if(kind==='delta_hover')assert(r.text.some(t=>t.value==='Δ 10'),JSON.stringify(r.text));
          if(kind==='pie_hover')assert(r.text.some(t=>t.value==='Share: 100.0%'),JSON.stringify(r.text));
          if(kind==='state_hover')assert(r.pixels.middle[3]>0,'hovered cell has no highlight');
          if(kind==='scatter_cloud')assert((r.counts.arc||0)>20,'cloud interior was reduced to extremes');
          if(kind==='ohlc_hover') {
            const guides=r.strokes.filter(s=>s.path.length===2 && s.path[0].args[1]===s.path[1].args[1]);
            assert.equal(guides.length,4,JSON.stringify(guides));
            for (const py of r.layout.slice(4)) assert(guides.some(s=>Math.abs(s.path[0].args[1]-py)<1));
            assert.equal(guides.filter(s=>s.color.includes('0.22')).length,2,'wick guides must be faint');
          }
        });
      }
      if(dpr===1) for(const kind of ['scatter','stack','denseband','denseline']) {
        const small=await render(page,kind,400,180,1000),large=await render(page,kind,400,180,1000000);
        const full=await render(page,`${kind}_full`,400,180,100000);
        results.push(small,large,full);
        check(`${kind} visible-window command budget`,()=>{
          assert(!large.error,large.error);
          const cost=r=>Object.values(r.counts).reduce((a,b)=>a+b,0);
          assert(cost(large)<=cost(small)+100,`${cost(small)} -> ${cost(large)}`);
          assert(cost(large)<5000,`${cost(large)} calls for 101 visible samples`);
        });
        check(`${kind} full-domain command budget`,()=>{
          assert(!full.error,full.error);
          assert.equal(full.invalid.length,0);
          const cost=Object.values(full.counts).reduce((a,b)=>a+b,0);
          assert(cost<20000,`${cost} calls for 100000 visible samples`);
        });
      }
      await page.screenshot({path:path.join(out,`dpr-${dpr}.png`),fullPage:true});
      await page.close();
    }
  } finally { await browser.close(); server.close(); }
  fs.writeFileSync(path.join(out,'results.json'),JSON.stringify({results,failures},null,2));
  if(failures.length){for(const failure of failures)console.error(failure);process.exitCode=1;}
  else console.log(`Graphtron browser regressions passed (${results.length} fixtures across five DPRs).`);
})().catch(error=>{console.error(error);server.close();process.exitCode=1;});
