// 验证 Studio 在没有 Toolchain、Sidecar 或相邻 Toolchain repository 的环境中
// 仍能完成全部核心流程。
// 用法：node scripts/verify-studio-standalone.mjs

import { execFile, spawn } from 'node:child_process';
import { promisify } from 'node:util';
import { existsSync } from 'node:fs';
import { mkdtemp, readFile, realpath, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createInterface } from 'node:readline';

const execFileAsync = promisify(execFile);

const checks = [];
function record(name, ok, detail) {
  checks.push({ name, ok, detail });
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? ` — ${detail}` : ''}`);
}

function cliPath() {
  const configured = process.env.STUDIO_CLI_BIN;
  if (configured) return resolve(configured);
  const release = resolve('target/release/studio-cli');
  const debug = resolve('target/debug/studio-cli');
  return existsSync(release) ? release : debug;
}

async function runCli(args) {
  try {
    await execFileAsync(cliPath(), args, { cwd: resolve('.') });
    return true;
  } catch (error) {
    console.log('  cli 调用失败:', String(error.stderr ?? error.message).trim().split('\n')[0]);
    return false;
  }
}

function waitForServer(child) {
  return new Promise((resolvePromise, reject) => {
    const reader = createInterface({ input: child.stdout });
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
    child.on('exit', (code) => {
      clearTimeout(timer);
      reject(new Error(`server exited with ${code}`));
    });
  });
}

async function main() {
  const root = resolve('.');
  const workspace = await mkdtemp(join(tmpdir(), 'ice3-independence-'));
  // 明确清掉可能让 Studio 找到已退役外部进程的变量。
  const env = { ...process.env };
  for (const key of [
    'STUDIO_TOOLCHAIN_CLI',
    'ICE3_TOOLCHAIN_CLI',
    'STUDIO_APP_ROOT',
    'ICE3_JT_CONVERTER',
  ]) delete env[key];
  env.STUDIO_SESSION_TOKEN = 'independence-check';

  const server = spawn(
    cliPath(),
    ['serve', '--workspace', workspace, '--project', 'independence', '--port', '0', '--write-mode', 'autonomous'],
    { cwd: root, env, stdio: ['ignore', 'pipe', 'inherit'] },
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
      return envelope;
    };

    // 1. 已退役 asset driver 不再出现在 capability，核心方法继续可用
    const capabilities = await rpc('system.capabilities');
    record(
      '已退役 asset driver 不在 capability 中',
      capabilities.result && !('toolchain' in capabilities.result),
    );
    record(
      'export.runtimePreview 在能力清单中',
      (capabilities.result?.methods ?? []).includes('export.runtimePreview'),
      `${(capabilities.result?.methods ?? []).length} 个方法可用`,
    );

    // 2. 建一个有真实几何的场景
    const applied = await rpc('document.apply', {
      expectedRevision: 0,
      dryRun: false,
      idempotencyKey: 'independence-1',
      operations: [{
        type: 'component.define',
        definition: {
          id: 'component-000001',
          name: 'Clarifier',
          parameters: [],
          parts: [
            { id: 'base', content: { kind: 'cylinder', radius: 8, height: 0.4, segments: 24 } },
            {
              id: 'wall',
              transform: { translation: { x: 0, y: 2, z: 0 } },
              content: { kind: 'cylinder', radius: 8, height: 4, segments: 24 },
            },
          ],
          ports: [],
          metadata: {},
        },
      }, {
        type: 'instance.create',
        node_id: 'node-000001',
        parent_id: null,
        name: 'SC-101',
        transform: { translation: { x: 0, y: 0, z: 0 }, rotation: [0, 0, 0, 1], scale: { x: 1, y: 1, z: 1 } },
        instance: {
          definitionId: 'component-000001',
          parameters: {},
          overrides: {},
          metadata: { tag: 'SC-101' },
        },
        material_id: null,
        metadata: {},
      }],
    });
    record('document.apply 成功', applied.result?.revision === 1, `revision=${applied.result?.revision}`);

    // 3. 预览必须直接由 Studio 内核产出
    const preview = await rpc('export.runtimePreview', {});
    const previewPath = preview.result?.previewPath;
    let previewOk = false;
    let previewBytes = 0;
    if (previewPath) {
      const info = await stat(previewPath).catch(() => null);
      previewBytes = info?.size ?? 0;
      const head = info ? (await readFile(previewPath)).subarray(0, 4).toString('binary') : '';
      previewOk = head === 'glTF' && previewBytes > 1000;
    }
    const expectedPreview = join(workspace, 'independence/cache/preview/00000001.glb');
    const previewIsOwned = previewPath !== undefined
      && await realpath(previewPath).then(
        (actual) => realpath(expectedPreview).then((expected) => actual === expected),
      ).catch(() => false);
    record(
      '预览由 Studio 内核直接产出',
      previewOk && previewIsOwned,
      `${previewBytes} 字节 · ${previewPath}`,
    );
    record(
      '未创建已退役 toolchain cache',
      !(await stat(join(workspace, 'independence/cache/toolchain')).catch(() => null)),
    );

    // 4. 源 GLB 导出
    const source = await rpc('export.sourceGlb', {});
    const sourceOk = Boolean(source.result?.sourcePath)
      && (await stat(source.result.sourcePath).catch(() => null))?.size > 1000;
    record('导出源 GLB', sourceOk, source.result?.sourcePath ? '文件存在' : '缺少路径');

    // 5. 场景查询（不依赖几何后端）
    const document = await rpc('document.get');
    record(
      'document.get 返回场景树',
      document.result?.scene?.nodes?.length === 1,
      `${document.result?.scene?.nodes?.length} 个节点`,
    );

    // 6. 求值统计
    const stats = await rpc('evaluation.stats');
    record(
      'evaluation.stats 可用',
      Number(stats.result?.effectiveTriangles) > 0,
      `${stats.result?.effectiveTriangles} 三角形`,
    );

    // 7. 候选预览：使用 Studio-owned candidate cache
    const created = await rpc('changeset.create', {
      baseRevision: 1,
      title: 'independence candidate',
    });
    const changeSetId = created.result?.id;
    const added = await rpc('changeset.addOperations', {
      changeSetId,
      expectedVersion: created.result?.version,
      entries: [{
        source: { kind: 'agent', label: 'independence' },
        operation: {
          type: 'node.update',
          node_id: 'node-000001',
          name: 'SC-101 renamed',
        },
      }],
    });
    record(
      'changeset 流程可用',
      Boolean(changeSetId) && added.result?.id === changeSetId,
      `changeSet=${changeSetId}`,
    );
    const evaluated = await rpc('changeset.evaluate', {
      changeSetId,
      expectedVersion: added.result?.version,
      includeCandidate: false,
    });
    record(
      '候选 ChangeSet 求值可用',
      evaluated.result?.status === 'ready',
      `digest=${evaluated.result?.candidateDigest}`,
    );
    const candidateDigest = evaluated.result?.candidateDigest;
    const candidate = await rpc('export.runtimePreview', {
      candidateDigest: candidateDigest ?? null,
    });
    const candidatePath = candidateDigest
      ? join(workspace, 'independence/cache/preview/candidates', `${candidateDigest}.glb`)
      : null;
    record(
      '候选预览写入 Studio-owned cache',
      candidate.result !== undefined
        && candidatePath !== null
        && (await stat(candidatePath).catch(() => null))?.size > 1000,
      candidate.error ? `error=${candidate.error.diagnosticCode}` : candidatePath ?? 'missing digest',
    );

    // 8. 全流程后仍不暴露已退役 driver
    const after = await rpc('system.capabilities');
    record(
      '全流程后仍不暴露已退役 driver',
      after.result && !('toolchain' in after.result),
      '说明未隐式启动外部 asset process',
    );

    // 9. 容器往返：pack / unpack 不依赖外部进程
    const projectDir = join(workspace, 'independence');
    const archive = join(workspace, 'independence.ice3');
    const restored = join(workspace, 'restored');
    const packOk = await runCli(['pack', '--project', projectDir, '--output', archive]);
    const unpackOk = packOk && await runCli(['unpack', archive, '--into', restored]);
    const archiveSize = (await stat(archive).catch(() => null))?.size ?? 0;
    const restoredOk = unpackOk
      && (await stat(join(restored, 'project.json')).catch(() => null)) !== null;
    record('打包为 .ice3', packOk && archiveSize > 0, `${archiveSize} 字节`);
    record('从 .ice3 还原工程', restoredOk, restoredOk ? 'project.json 存在' : '还原失败');

    console.log('\n工程目录:', workspace);
    const failed = checks.filter((check) => !check.ok);
    console.log(`\n${checks.length - failed.length}/${checks.length} 项通过`);
    if (failed.length > 0) {
      process.exitCode = 1;
    }
  } finally {
    server.kill('SIGTERM');
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
