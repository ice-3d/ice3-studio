// Agent 视觉自检截图通道：驱动用户已安装的 Google Chrome 渲染 Web Viewer 并输出 PNG。
// 形态是脚本而非 RPC——Studio Server 不渲染几何，截图与 E2E 共用 Playwright + Chrome。

import { spawn } from 'node:child_process';
import { mkdir } from 'node:fs/promises';
import { dirname, isAbsolute, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const studioRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');

export function parseArguments(argv) {
  const options = {
    url: null,
    baseUrl: 'http://127.0.0.1:8120',
    token: null,
    view: null,
    target: null,
    out: 'output/agent-screenshot.png',
    width: 1600,
    height: 900,
    wait: 3000,
    project: null,
    workspace: 'projects',
    cli: 'target/debug/studio-cli',
    channel: process.env.PLAYWRIGHT_CHANNEL ?? 'chrome',
  };
  for (let index = 0; index < argv.length; index += 1) {
    const flag = argv[index];
    const value = argv[index + 1];
    const needsValue = () => {
      if (index + 1 >= argv.length) {
        throw new Error(`missing value for ${flag}`);
      }
      return value;
    };
    switch (flag) {
      case '--url': options.url = needsValue(); index += 1; break;
      case '--base-url': options.baseUrl = needsValue(); index += 1; break;
      case '--token': options.token = needsValue(); index += 1; break;
      case '--view': options.view = needsValue(); index += 1; break;
      case '--target': options.target = needsValue(); index += 1; break;
      case '--out': options.out = needsValue(); index += 1; break;
      case '--width': options.width = Number(needsValue()); index += 1; break;
      case '--height': options.height = Number(needsValue()); index += 1; break;
      case '--wait': options.wait = Number(needsValue()); index += 1; break;
      case '--project': options.project = needsValue(); index += 1; break;
      case '--workspace': options.workspace = needsValue(); index += 1; break;
      case '--cli': options.cli = needsValue(); index += 1; break;
      case '--channel': options.channel = needsValue(); index += 1; break;
      default: throw new Error(`unknown option ${flag}`);
    }
  }
  if (options.url && (options.view || options.target || options.token)) {
    throw new Error('--url cannot be combined with --view/--target/--token');
  }
  return options;
}

function parseTriple(name, raw) {
  if (raw === null || raw === undefined) return null;
  const parts = String(raw).split(',').map((part) => Number(part));
  if (parts.length !== 3 || parts.some((part) => !Number.isFinite(part))) {
    throw new Error(`${name} must be three comma-separated finite numbers, got: ${raw}`);
  }
  return parts;
}

export function buildScreenshotTarget(options) {
  if (options.url) {
    return { url: options.url, baseUrl: null };
  }
  const view = parseTriple('--view', options.view);
  const target = parseTriple('--target', options.target);
  const url = new URL(options.baseUrl ?? 'http://127.0.0.1:8120');
  if (options.token) url.searchParams.set('token', options.token);
  if (view) url.searchParams.set('view', view.join(','));
  if (target) url.searchParams.set('target', target.join(','));
  return { url: url.toString(), baseUrl: url.origin };
}

async function startTempServer(options) {
  const cli = isAbsolute(options.cli)
    ? options.cli
    : resolve(process.cwd(), options.cli);
  const workspace = isAbsolute(options.workspace)
    ? options.workspace
    : resolve(process.cwd(), options.workspace);
  const server = spawn(cli, [
    'serve',
    '--workspace', workspace,
    '--project', options.project,
    '--port', '0',
    '--write-mode', 'review-required',
  ], {
    stdio: ['ignore', 'pipe', 'inherit'],
    env: {
      ...process.env,
      // serve 需要显式指向 viewer 静态目录才会托管 Web 界面。
      STUDIO_VIEWER_DIR: resolve(studioRoot, 'apps/viewer/dist'),
    },
  });
  const lines = [];
  // server 启动打印两行：监听地址与写入令牌。
  await new Promise((resolvePromise, rejectPromise) => {
    const timer = setTimeout(
      () => rejectPromise(new Error('temporary server did not start within 60s')),
      60_000,
    );
    server.stdout.on('data', (chunk) => {
      for (const line of String(chunk).split('\n')) {
        const text = line.trim();
        if (!text) continue;
        lines.push(text);
        if (lines.length >= 2) {
          clearTimeout(timer);
          resolvePromise();
        }
      }
    });
    server.once('exit', (code) => {
      clearTimeout(timer);
      rejectPromise(new Error(`temporary server exited with ${code}`));
    });
  });
  return { server, baseUrl: `http://${lines[0]}`, token: lines[1] ?? null };
}

async function main() {
  const options = parseArguments(process.argv.slice(2));
  let server = null;
  try {
    if (options.project) {
      const started = await startTempServer(options);
      server = started.server;
      options.baseUrl = started.baseUrl;
      if (!options.token && started.token) options.token = started.token;
    }
    const target = buildScreenshotTarget(options);
    // playwright 安装在 apps/viewer 下（与 E2E 同一份），从仓库根脚本显式定位。
    const { chromium } = await import(
      new URL('../apps/viewer/node_modules/playwright/index.mjs', import.meta.url).href
    );
    const browser = await chromium.launch({
      channel: options.channel === 'none' ? undefined : options.channel,
    });
    try {
      const page = await browser.newPage({
        viewport: { width: options.width, height: options.height },
      });
      page.on('console', (message) => {
        if (message.type() === 'error') {
          process.stderr.write(`[console] ${message.text().slice(0, 200)}\n`);
        }
      });
      page.on('response', (response) => {
        if (response.status() >= 400) {
          process.stderr.write(
            `[http ${response.status()}] ${response.request().method()} ${response.url()}\n`,
          );
        }
      });
      await page.goto(target.url, { waitUntil: 'load', timeout: 60_000 });
      const canvas = page.locator('canvas[data-layer="scene"]');
      await canvas.waitFor({ state: 'visible', timeout: 30_000 });
      // 大 reference 资产流式加载有明确的 ready 信号；普通 preview 直接 settle。
      await page
        .locator('section.viewport[data-reference-state="ready"]')
        .waitFor({ state: 'visible', timeout: 10_000 })
        .catch(() => {});
      await page.waitForTimeout(options.wait);
      const out = isAbsolute(options.out)
        ? options.out
        : resolve(process.cwd(), options.out);
      await mkdir(dirname(out), { recursive: true });
      await page.screenshot({ path: out });
      console.log(out);
    } finally {
      await browser.close().catch(() => {});
    }
  } finally {
    if (server) {
      server.kill('SIGTERM');
      await new Promise((resolvePromise) => {
        const timer = setTimeout(() => server.kill('SIGKILL'), 3000);
        server.once('exit', () => {
          clearTimeout(timer);
          resolvePromise();
        });
      });
    }
  }
}

if (process.argv[1] && import.meta.url === new URL(`file://${process.argv[1]}`).href) {
  main().catch((error) => {
    process.stderr.write(`${error.message ?? error}\n`);
    process.exitCode = 1;
  });
}
