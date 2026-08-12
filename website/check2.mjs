import { chromium } from 'playwright';
const b = await chromium.launch();
const px = (p) => p.evaluate(() => {
  const g = document.querySelector('canvas.dots').getContext('2d');
  const d = g.getImageData(0, 0, 300, 200).data;
  let s = 0; for (let i = 3; i < d.length; i += 4) s += d[i] * (i % 7 + 1);
  return s;
});

const p = await b.newPage({ viewport: { width: 1440, height: 900 } });
await p.goto('http://localhost:4321/', { waitUntil: 'networkidle' });
await p.waitForTimeout(600);
const a = await px(p); await p.waitForTimeout(1000); const c = await px(p);
console.log('normal: animating   =', a !== c);

await p.evaluate(() => window.scrollTo(0, 5000));
await p.waitForTimeout(800);
const s1 = await px(p); await p.waitForTimeout(1000); const s2 = await px(p);
console.log('normal: paused off-screen =', s1 === s2);

const q = await b.newPage({ viewport: { width: 1440, height: 900 }, reducedMotion: 'reduce' });
await q.goto('http://localhost:4321/', { waitUntil: 'networkidle' });
await q.waitForTimeout(1200);
console.log('reduce: media matches     =', await q.evaluate(() => matchMedia('(prefers-reduced-motion: reduce)').matches));
const r1 = await px(q); await q.waitForTimeout(1200); const r2 = await px(q);
console.log('reduce: static            =', r1 === r2, '| dots drawn =', r1 > 0);
await b.close();
