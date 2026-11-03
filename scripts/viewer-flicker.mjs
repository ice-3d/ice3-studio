// 对 ice3-studio Web Viewer 的闪烁连拍：匀速拖拽旋转中截帧供往复跳变分析。
// 用法: node scripts/viewer-flicker.mjs <baseUrl> <outDir>
import { mkdir, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';

const baseUrl = process.argv[2] ?? 'http://127.0.0.1:8182';
const outDir = process.argv[3] ?? resolve(process.cwd(), 'output/viewer-flicker');
const reference = process.argv[4] !== 'preview';
const mode = process.argv[5] ?? 'slow';
const FRAMES = 24;

const { chromium } = await import(
  new URL('../apps/viewer/node_modules/playwright/index.mjs', import.meta.url).href
);
const browser = await chromium.launch({ channel: 'chrome' });
try {
  const page = await browser.newPage({ viewport: { width: 1600, height: 900 } });
  const view = process.argv[6] ? `&view=${process.argv[6]}` : '';
  const url = reference
    ? `${baseUrl}/?token=agent-live-test&reference=glb${view}`
    : `${baseUrl}/?token=agent-live-test${view}`;
  await page.goto(url, { waitUntil: 'load', timeout: 60_000 });
  await page.locator('canvas[data-layer="scene"]')
    .waitFor({ state: 'visible', timeout: 30_000 });
  await page.waitForTimeout(8000);

  await mkdir(outDir, { recursive: true });
  const canvas = await page.locator('canvas[data-layer="scene"]').boundingBox();
  const cx = canvas.x + canvas.width / 2;
  const cy = canvas.y + canvas.height / 2;

  const frames = [];
  for (let index = 0; index < FRAMES; index += 1) {
    if (mode === 'still') {
      // 静止基线
    } else if (index === 0) {
      await page.mouse.move(cx, cy);
      await page.mouse.down();
    }
    if (mode === 'fast') {
      await page.mouse.move(cx + 30 * (index + 1), cy + 4 * index, { steps: 2 });
    } else if (mode !== 'still') {
      await page.mouse.move(cx + 5 * (index + 1), cy + index, { steps: 1 });
    }
    await page.waitForTimeout(160);
    frames.push(await page.screenshot({ type: 'png' }));
  }
  if (mode !== 'still') await page.mouse.up();
  for (let index = 0; index < frames.length; index += 1) {
    await writeFile(
      resolve(outDir, `frame-${String(index).padStart(2, '0')}.png`),
      frames[index],
    );
  }
  console.log(`${FRAMES} frames captured to ${outDir}`);
} finally {
  await browser.close().catch(() => {});
}
