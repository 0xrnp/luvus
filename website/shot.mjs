import { chromium } from 'playwright';
const b = await chromium.launch();
const p = await b.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 2 });
await p.goto('http://localhost:4321/', { waitUntil: 'networkidle' });
await p.waitForTimeout(600);
await p.locator('header.hero').screenshot({ path: '/tmp/hero.png' });
await b.close();
console.log('ok');
