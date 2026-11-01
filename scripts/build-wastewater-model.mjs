// 污水处理厂资产命令入口。模型内容由 scripts/wastewater-plant/ 下的模块提供。

import { spawn } from 'node:child_process';
import { mkdir, mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createInterface } from 'node:readline';
import { buildPlan } from './wastewater-plant/site-plan.mjs';

function waitForServer(server) {
  return new Promise((resolvePromise, reject) => {
    const reader = createInterface({ input: server.stdout });
    const lines = [];
    const timer = setTimeout(() => reject(new Error('server did not start')), 120_000);
    reader.on('line', (line) => {
      const text = line.trim();
      if (!text) return;
      lines.push(text);
      if (lines.length === 2) {
        clearTimeout(timer);
        resolvePromise({ address: lines[0], token: lines[1] });
      }
    });
    server.on('exit', (code) => {
      clearTimeout(timer);
      reject(new Error(`server exited with ${code}`));
    });
  });
}

async function main() {
  const root = resolve('.');
  const explicitWorkspace = process.argv.includes('--workspace')
    ? resolve(process.argv[process.argv.indexOf('--workspace') + 1])
    : null;
  const workspace = explicitWorkspace ?? await mkdtemp(join(tmpdir(), 'ice3-wwtp-built-'));
  if (explicitWorkspace) {
    await mkdir(explicitWorkspace, { recursive: true });
  }
  const plan = buildPlan('wastewater-plant');
  const server = spawn(
    resolve('target/release/studio-cli'),
    ['serve', '--workspace', workspace, '--project', 'bootstrap', '--port', '0', '--write-mode', 'autonomous'],
    {
      cwd: root,
      env: {
        ...process.env,
        STUDIO_SESSION_TOKEN: 'agent-wastewater-plant',
        STUDIO_VIEWER_DIR: resolve('apps/viewer/dist'),
      },
      stdio: ['ignore', 'pipe', 'inherit'],
    },
  );

  try {
    const { address, token } = await waitForServer(server);
    let requestId = 0;
    const rpc = async (method, params = {}) => {
      requestId += 1;
      const response = await fetch(`http://${address}/rpc`, {
        method: 'POST',
        headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json' },
        body: JSON.stringify({ jsonrpc: '2.0', id: requestId, method, params }),
      });
      const envelope = await response.json();
      if (envelope.error) throw new Error(`${method}: ${JSON.stringify(envelope.error)}`);
      return envelope.result;
    };

    if (explicitWorkspace) {
      await rm(join(workspace, plan.projectId), { recursive: true, force: true }).catch(() => {});
    }
    await rpc('project.create', { projectId: plan.projectId });
    const committed = await rpc('document.apply', {
      expectedRevision: 0,
      dryRun: false,
      idempotencyKey: 'wastewater-plant-1',
      operations: plan.operations,
    });
    const stats = await rpc('evaluation.stats');
    const exported = await rpc('export.sourceGlb');
    console.log(JSON.stringify({
      url: `http://${address}/?token=${token}`,
      revision: committed.revision,
      operations: plan.operations.length,
      stats,
      summary: plan.summary,
      sourceGlb: exported.sourcePath,
    }, null, 2));
    const keepAliveMs = Number(process.env.KEEP_ALIVE_MS ?? 0);
    if (keepAliveMs > 0) {
      await new Promise((resolvePromise) => setTimeout(resolvePromise, keepAliveMs));
    }
  } finally {
    server.kill('SIGTERM');
    await rm(join(workspace, 'bootstrap'), { recursive: true, force: true }).catch(() => {});
  }
}

if (process.argv[1] && import.meta.url === `file://${process.argv[1]}`) {
  main().catch((error) => {
    console.error(error);
    process.exitCode = 1;
  });
}

export { buildPlan };
