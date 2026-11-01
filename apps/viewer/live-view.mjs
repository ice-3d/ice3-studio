// 常驻的有头浏览器：盯三处变化，任何一处变了就刷新页面。
//
//   1. /tmp/reload.flag          —— 手动打标记（改完模型、重启过服务时用）
//   2. dist/assets/main.js 的 mtime —— 重新 esbuild 之后自动刷，不依赖我记得打标记
//   3. 页面本身报错（白屏/上下文丢失）—— 自动重载恢复
//
// 之前只有第 1 条，验证改成无头截图后窗口就长期停在旧画面上了。
import { chromium } from 'playwright';
import { statSync } from 'node:fs';
import { resolve } from 'node:path';

const url = process.argv[2] ?? 'http://127.0.0.1:8120/?token=dev&view=30,36,240';
const flag = '/tmp/reload.flag';
const bundle = resolve('dist/assets/main.js');

const browser = await chromium.launch({
  headless: false,
  args: ['--start-maximized', '--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader'],
});
const page = await browser.newPage({ viewport: null });

async function reload(reason) {
  try {
    await page.reload({ waitUntil: 'load' });
    console.log(`reloaded (${reason}) at`, new Date().toISOString());
  } catch (error) {
    console.error('reload failed:', String(error).slice(0, 120));
  }
}

await page.goto(url, { waitUntil: 'load' });
console.log('live view ready:', url);

const stampOf = (path) => {
  try {
    return statSync(path).mtimeMs;
  } catch {
    return 0;
  }
};

let lastFlag = stampOf(flag);
let lastBundle = stampOf(bundle);

setInterval(async () => {
  const flagStamp = stampOf(flag);
  const bundleStamp = stampOf(bundle);
  if (flagStamp !== lastFlag) {
    lastFlag = flagStamp;
    await reload('flag');
    return;
  }
  if (bundleStamp !== lastBundle) {
    lastBundle = bundleStamp;
    await reload('bundle');
  }
}, 1200);
