import { chromium } from 'playwright';

const url = process.argv[2] ?? 'http://127.0.0.1:8120/?token=dev&view=20,34,540';
const out = process.argv[3] ?? '/tmp/pw-shot.png';

const browser = await chromium.launch({
  args: ['--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader'],
});
const page = await browser.newPage({ viewport: { width: 1600, height: 900 } });
page.on('console', (message) => {
  if (message.type() === 'error') console.log('[console]', message.text().slice(0, 160));
});
await page.goto(url, { waitUntil: 'load' });
await page.waitForTimeout(9000);
await page.screenshot({ path: out });
console.log('saved', out);
await browser.close();
