// Build with `dx build --web` in graphtron-demo, then run with Playwright available.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const http = require('node:http');
const { chromium } = require('playwright');
const repo = path.resolve(__dirname, '../../..');
const root = path.join(repo, 'target/dx/graphtron-demo/debug/web/public');
const out = path.join(repo, 'target/graphtron-browser');
fs.mkdirSync(out, { recursive: true });
const server = http.createServer((req, res) => {
  const pathname = new URL(req.url, 'http://localhost').pathname;
  const file = path.resolve(root, '.' + (['/', '/rrdtool', '/rrdtool/'].includes(pathname) ? '/index.html' : decodeURIComponent(pathname)));
  if (!file.startsWith(root + path.sep) || !fs.existsSync(file) || !fs.statSync(file).isFile()) { res.writeHead(404); res.end(); return; }
  res.setHeader('Content-Type', { '.html': 'text/html', '.js': 'text/javascript', '.wasm': 'application/wasm', '.css': 'text/css' }[path.extname(file)] || 'application/octet-stream');
  fs.createReadStream(file).pipe(res);
});
(async () => {
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const browser = await chromium.launch({ headless: true });
  try {
    for (const dpr of [1, 2]) {
      const page = await browser.newPage({ viewport: { width: 1100, height: 900 }, deviceScaleFactor: dpr, acceptDownloads: true });
      const errors = [];
      page.on('pageerror', e => errors.push(e.message));
      await page.goto(`http://127.0.0.1:${server.address().port}/rrdtool`);
      await page.getByRole('heading', { name: '2. Interactive graphs', exact: true }).waitFor();
      await page.evaluate(() => document.fonts.ready);
      const idle = () => page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
      await idle();
      assert.equal(await page.locator('.rrd-static').count(), 3);
      assert.equal(await page.locator('.rrd-interactive').count(), 3);
      const pixels = selector => page.locator(selector).evaluate(el => el.toDataURL());
      const snapshots = [];
      for (let i = 0; i < 3; i++) {
        const before = await pixels(`#rrd-static-${i} canvas` + ':first-of-type');
        snapshots.push(before);
        assert.equal(await pixels(`#rrd-interactive-${i} canvas:first-of-type`), before, 'Static and interactive data layers differ at rest');
        await page.locator(`#rrd-static-${i} .rrd-image`).screenshot({ path: path.join(out, `rrd-static-${i}-dpr${dpr}.png`) });
      }
      const colors = await page.locator('#rrd-static-0 canvas').first().evaluate(canvas => {
        const c = canvas.getContext('2d'), d = devicePixelRatio;
        return [[4,4],[0,0],[759,285],[101,38]].map(([x,y])=>Array.from(c.getImageData(x*d,y*d,1,1).data));
      });
      assert.deepEqual(colors[0], [242,242,242,255]);
      assert.deepEqual(colors[1], [207,207,207,255]);
      assert.deepEqual(colors[2], [158,158,158,255]);
      assert.deepEqual(colors[3], [255,255,255,255]);
      const toggle = page.getByRole('button', { name: 'Dark mode', exact: true });
      await toggle.click(); await idle();
      assert.equal(await toggle.getAttribute('aria-pressed'), 'true');
      for (let i = 0; i < 3; i++) {
        const dark = await pixels(`#rrd-static-${i} canvas:first-of-type`);
        assert.notEqual(dark, snapshots[i]);
        assert.equal(await pixels(`#rrd-interactive-${i} canvas:first-of-type`), dark);
      }
      const darkColors = await page.locator('#rrd-static-0 canvas').first().evaluate(canvas => {
        const c = canvas.getContext('2d'), d = devicePixelRatio;
        return [[4,4],[101,38],[100,200]].map(([x,y])=>Array.from(c.getImageData(x*d,y*d,1,1).data));
      });
      assert.deepEqual(darkColors[0], [0,0,0,255]);
      assert.deepEqual(darkColors[1], [0,0,0,255]);
      assert(darkColors[2][2] > darkColors[2][0], 'Dark trace should use the cyan/blue spectrum');
      await page.locator('#rrd-static-0 .rrd-image').screenshot({ path: path.join(out, `rrd-dark-dpr${dpr}.png`) });
      await page.getByRole('button', { name: 'Last 6 hours', exact: true }).click(); await idle();
      const darkWindow = await page.locator('.rrd-window').innerText();
      await toggle.click(); await idle();
      assert.equal(await page.locator('.rrd-window').innerText(), darkWindow, 'Theme toggle reset the zoom');
      await page.getByRole('button', { name: 'Reset 24 hours', exact: true }).click(); await idle();
      for (let i = 0; i < 3; i++) {
        assert.equal(await pixels(`#rrd-static-${i} canvas:first-of-type`), snapshots[i], 'Light theme did not restore exactly');
        assert.equal(await pixels(`#rrd-interactive-${i} canvas:first-of-type`), snapshots[i]);
      }
      await page.locator('#rrd-static-0 canvas').nth(1).hover(); await idle();
      assert.equal(await pixels('#rrd-static-0 canvas:first-of-type'), snapshots[0]);
      assert.equal(await page.locator('.rrd-static [tabindex="0"]').count(), 0);

      const card = page.locator('#rrd-interactive-0');
      await card.scrollIntoViewIfNeeded();
      const overlay = card.locator('canvas').nth(1);
      const blank = await overlay.evaluate(c => c.toDataURL());
      const box = await overlay.boundingBox();
      await page.mouse.move(box.x + 340, box.y + 100); await idle();
      assert.notEqual(await overlay.evaluate(c => c.toDataURL()), blank, 'No interactive hover overlay');
      assert.equal(await pixels('#rrd-interactive-0 canvas:first-of-type'), snapshots[0], 'Hover repainted data geometry');
      await overlay.click({ position: { x: 340, y: 100 } });
      await card.locator('.graphtron-tooltip-inspector').waitFor();
      await card.locator('[role=region]').focus(); await page.keyboard.press('Escape');
      await card.locator('.graphtron-tooltip-inspector').waitFor({ state: 'hidden' });
      const initialWindow = await page.locator('.rrd-window').innerText();
      await card.locator('[role=region]').focus();
      for (let i = 0; i < 8; i++) await page.keyboard.press('-');
      await idle();
      assert.equal(await page.locator('.rrd-window').innerText(), initialWindow, 'Zoom-out escaped loaded data');
      for (let i = 0; i < 40; i++) await page.keyboard.press('+');
      await idle();
      const minimumWindow = await page.locator('.rrd-window').innerText();
      assert.notEqual(minimumWindow, initialWindow);
      for (let i = 0; i < 12; i++) await page.keyboard.press('+');
      await idle();
      assert.equal(await page.locator('.rrd-window').innerText(), minimumWindow, 'Repeated zoom-in escaped minimum span');
      await page.getByRole('button', { name: 'Reset 24 hours', exact: true }).click(); await idle();
      await card.locator('[role=region]').focus();
      await page.keyboard.press('ArrowLeft'); await page.keyboard.press('ArrowRight'); await idle();
      assert.equal(await page.locator('.rrd-window').innerText(), initialWindow, 'Full-extent pan escaped loaded data');
      await page.getByRole('button', { name: 'Last 6 hours', exact: true }).click(); await idle();
      assert.notEqual(await page.locator('.rrd-window').innerText(), initialWindow);
      for (let i=0;i<3;i++) {
        assert.notEqual(await pixels(`#rrd-interactive-${i} canvas:first-of-type`), snapshots[i], 'Shared zoom did not redraw a graph');
        assert.equal(await pixels(`#rrd-static-${i} canvas:first-of-type`), snapshots[i], 'Static graph followed interactive zoom');
      }
      await page.getByRole('button', { name: 'Reset 24 hours', exact: true }).click(); await idle();
      assert.equal(await page.locator('.rrd-window').innerText(), initialWindow);
      await card.scrollIntoViewIfNeeded();
      const dragBox = await overlay.boundingBox();
      await page.mouse.move(dragBox.x + 180, dragBox.y + 110);
      await page.mouse.down(); await page.mouse.move(dragBox.x + 500, dragBox.y + 110, { steps: 8 }); await page.mouse.up(); await idle();
      assert.notEqual(await page.locator('.rrd-window').innerText(), initialWindow, 'Drag did not zoom');
      await page.getByRole('button', { name: 'Reset 24 hours', exact: true }).click(); await idle();
      await card.scrollIntoViewIfNeeded(); await overlay.hover(); await page.mouse.wheel(0, -200);
      await page.waitForFunction(initial => document.querySelector('.rrd-window').textContent !== initial, initialWindow);
      await page.getByRole('button', { name: 'Reset 24 hours', exact: true }).click(); await idle();
      await card.locator('[role=region]').focus(); await page.keyboard.press('+'); await idle();
      assert.notEqual(await page.locator('.rrd-window').innerText(), initialWindow, 'Keyboard did not zoom');
      await page.getByRole('button', { name: 'Reset 24 hours', exact: true }).click(); await idle();
      const legend = card.locator('.graphtron-legend');
      await legend.getByRole('button').first().click(); await idle();
      assert.notEqual(await pixels('#rrd-interactive-0 canvas:first-of-type'), snapshots[0], 'Legend toggle did not update image');
      await legend.getByRole('button').first().click(); await idle();
      assert.equal(await pixels('#rrd-interactive-0 canvas:first-of-type'), snapshots[0]);
      const download = page.waitForEvent('download');
      await page.locator('#rrd-static-0').getByRole('button', { name: 'Download PNG' }).click();
      assert.equal((await download).suggestedFilename(), 'graphtron-rrd-0.png');
      await page.setViewportSize({ width: 390, height: 844 }); await idle();
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1), 'Page overflows on mobile');
      assert.deepEqual(errors, []);
      await page.close();
    }
    console.log('RRD page passed at DPR 1 and 2: identical static/interactive images, palette, hover, freeze, drag/wheel/keyboard/shared zoom, legends, PNG export, mobile.');
  } finally { await browser.close(); server.close(); }
})().catch(e => { console.error(e); server.close(); process.exitCode = 1; });
