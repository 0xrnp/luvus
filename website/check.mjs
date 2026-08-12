import { chromium } from 'playwright';
const b = await chromium.launch();
const p = await b.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 });
await p.goto('http://localhost:4321/', { waitUntil: 'networkidle' });
await p.waitForTimeout(500);

// frame rate of the rAF loop + whether pixels change
const fps = await p.evaluate(() => new Promise((res) => {
  let n = 0; const t = performance.now();
  const tick = () => { n++; performance.now() - t < 1000 ? requestAnimationFrame(tick) : res(n); };
  requestAnimationFrame(tick);
}));
const grab = async () => (await p.locator('canvas.dots').screenshot()).toString('base64');
const a = await grab(); await p.waitForTimeout(1200); const c = await grab();
console.log('rAF fps        :', fps);
console.log('canvas changed :', a !== c);

// is it a uniform translate, or independent motion?
const indep = await p.evaluate(() => {
  const cv = document.querySelector('canvas.dots');
  const g = cv.getContext('2d');
  const col = (x, y) => { const d = g.getImageData(x, y, 60, 60).data; let s = 0; for (let i = 3; i < d.length; i += 4) s += d[i]; return s; };
  return [col(200, 100), col(900, 300)];
});
console.log('alpha samples  :', indep.join(' / '), '(non-zero = dots drawn)');

// off-screen suspend
await p.evaluate(() => window.scrollTo(0, 4000));
await p.waitForTimeout(600);
const s1 = await grab(); await p.waitForTimeout(900); const s2 = await grab();
console.log('paused off-screen:', s1 === s2);

// reduced motion
const p2 = await b.newPage({ viewport: { width: 1440, height: 900 }, reducedMotion: 'reduce' });
await p2.goto('http://localhost:4321/', { waitUntil: 'networkidle' });
await p2.waitForTimeout(400);
const r1 = (await p2.locator('canvas.dots').screenshot()).toString('base64');
await p2.waitForTimeout(1200);
const r2 = (await p2.locator('canvas.dots').screenshot()).toString('base64');
console.log('reduced-motion static:', r1 === r2, '(dots still drawn:', r1.length > 2000, ')');
await b.close();
