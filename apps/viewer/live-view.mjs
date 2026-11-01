// 常驻的有头浏览器：监听 /tmp/reload.flag，文件变化就刷新页面。
// 这样模型每次改动都能立刻反映到屏幕上的窗口里。
import { chromium } from 'playwright';
import { statSync } from 'node:fs';

const url = process.argv[2] ?? 'http://127.0.0.1:8120/?token=dev&view=30,36,240';
const flag = '/tmp/reload.flag';

const browser = await chromium.launch({
  headless: false,
  args: ['--start-maximized', '--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader'],
});
const page = await browser.newPage({ viewport: null });
await page.goto(url, { waitUntil: 'load' });
console.log('live view ready:', url);

let last = 0;
setInterval(async () => {
  try {
    const stamp = statSync(flag).mtimeMs;
    if (stamp !== last) {
      last = stamp;
      await page.reload({ waitUntil: 'load' });
      console.log('reloaded at', new Date().toISOString());
    }
  } catch {
    // 标志文件还没创建
  }
}, 1200);
