import { spawn } from 'node:child_process';
import { once } from 'node:events';
import {
  copyFile,
  mkdtemp,
  readFile,
  readdir,
  rm,
} from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { createInterface } from 'node:readline';

const args = process.argv.slice(2);
const index = args.indexOf('--bundle');
if (index < 0 || !args[index + 1]) {
  throw new Error('usage: verify-toolchain-sidecar --bundle DIR [--fixture FILE]');
}
const bundle = resolve(args[index + 1]);
const fixtureIndex = args.indexOf('--fixture');
const fixture = fixtureIndex < 0
  ? resolve('fixtures/source-import/gltf/minimal.glb')
  : resolve(args[fixtureIndex + 1]);
const manifest = JSON.parse(
  await readFile(join(bundle, 'manifest.json'), 'utf8'),
);
const workspace = await mkdtemp(join(tmpdir(), 'ice3-sidecar-live-'));
const sourcePath = join(workspace, 'source.glb');
await copyFile(fixture, sourcePath);

const child = spawn(join(bundle, manifest.entrypoint), [
  join(bundle, manifest.packageEntry),
  'sidecar',
  '--workspace',
  workspace,
], {
  cwd: workspace,
  env: { PATH: process.env.PATH ?? '' },
  stdio: ['pipe', 'pipe', 'pipe'],
});
const exitPromise = once(child, 'exit');
const pending = new Map();
let nextId = 1;
let stderr = '';
child.stderr.setEncoding('utf8');
child.stderr.on('data', (chunk) => {
  stderr += chunk;
});
child.once('exit', (code, signal) => {
  for (const [id, entry] of pending) {
    pending.delete(id);
    entry.reject(new Error(
      `Toolchain sidecar exited (${signal ?? code}): ${stderr.trim()}`,
    ));
  }
});
createInterface({ input: child.stdout }).on('line', (line) => {
  const message = JSON.parse(line);
  if (message.id === undefined) return;
  const entry = pending.get(message.id);
  if (!entry) return;
  pending.delete(message.id);
  entry.resolve(message);
});

function request(method, params) {
  const id = nextId;
  nextId += 1;
  return new Promise((resolveRequest, reject) => {
    pending.set(id, { resolve: resolveRequest, reject });
    child.stdin.write(`${JSON.stringify({
      jsonrpc: '2.0',
      id,
      method,
      params,
    })}\n`);
  });
}

try {
  const capabilities = await request('toolchain.capabilities', {});
  if (capabilities.error) throw new Error(capabilities.error.message);
  if (capabilities.result.contractVersion !== manifest.contractVersion) {
    throw new Error('sidecar contract does not match bundle manifest');
  }
  if (capabilities.result.packageVersion !== manifest.packageVersion) {
    throw new Error('sidecar package does not match bundle manifest');
  }
  const preview = await request('asset.preview', {
    sourcePath: 'source.glb',
    outputPath: 'cache/toolchain/preview/asset.glb',
    assetId: 'live-smoke',
    budget: 'unrestricted',
  });
  if (preview.error) throw new Error(preview.error.message);
  const validation = await request('asset.validate', {
    sourcePath: preview.result.primaryPath,
  });
  if (validation.error || validation.result.status !== 'pass') {
    throw new Error('live Toolchain preview failed validation');
  }
  const partitionId = nextId;
  const partition = request('asset.partition', {
    sourcePath: 'source.glb',
    outputDirectory: 'cache/toolchain/partition',
    assetId: 'live-smoke',
    maxTriangles: 500_000,
    maxBytes: 33_554_432,
  });
  child.stdin.write(`${JSON.stringify({
    jsonrpc: '2.0',
    method: '$/cancel',
    params: { requestId: partitionId },
  })}\n`);
  const partitionResult = await partition;
  if (
    partitionResult.error
    && partitionResult.error.data.code !== 'ICE3_TOOL_SIDECAR_CANCELLED'
  ) {
    throw new Error(partitionResult.error.message);
  }
  const entries = await readdir(join(workspace, 'cache'), { recursive: true });
  if (entries.some((path) => path.endsWith('.tmp'))) {
    throw new Error('temporary sidecar files remain after cancellation');
  }
  console.log(JSON.stringify({
    contractVersion: capabilities.result.contractVersion,
    packageVersion: capabilities.result.packageVersion,
    preview: preview.result.primaryPath,
    partitionCancelled: partitionResult.error?.data?.code
      === 'ICE3_TOOL_SIDECAR_CANCELLED',
  }, null, 2));
} finally {
  child.stdin.end();
  if (child.exitCode === null && child.signalCode === null) {
    child.kill('SIGTERM');
  }
  await exitPromise;
  if (stderr) process.stderr.write(stderr);
  await rm(workspace, { recursive: true, force: true });
}
