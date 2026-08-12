import { chromium } from 'playwright';
const b = await chromium.launch();
const p = await b.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 });
await p.goto('http://localhost:4321/', { waitUntil: 'networkidle' });
const box = await p.locator('.hero .dots').boundingBox();
console.log('dots box:', JSON.stringify(box));
const t = async (label) => {
  const shot = await p.locator('header.hero').screenshot();
  return shot;
};
const a = await t(); await p.waitForTimeout(4000); const c = await t();
console.log('frame bytes differ:', Buffer.compare(a, c) !== 0);
// read the live transform of each layer over time
const read = () => p.evaluate(() => {
  const d = document.querySelector('.hero .dots');
  const g = (el, pseudo) => getComputedStyle(el, pseudo).transform;
  return [g(d, '::before'), g(d, '::after'), g(d.querySelector('i'), null)];
});
const t1 = await read(); await p.waitForTimeout(3000); const t2 = await read();
t1.forEach((v, i) => console.log(`layer ${i}:  ${v}   ->   ${t2[i]}   moved=${v !== t2[i]}`));
await b.close();
