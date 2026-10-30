import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';

const fixtureArg = process.argv.indexOf('--fixture');
const fixture = fixtureArg >= 0 ? process.argv[fixtureArg + 1] : undefined;
if (!fixture) {
  throw new Error('usage: run-ai-fixture.mjs --fixture <directory>');
}

const workspace = await mkdtemp(join(tmpdir(), 'ice3-studio-ai-'));
const server = spawn(
  'cargo',
  ['run', '-q', '-p', 'studio-cli', '--', 'serve', '--workspace', workspace, '--project', 'wwtp-mini', '--port', '0'],
  { stdio: ['ignore', 'pipe', 'inherit'] },
);
const lines = createInterface({ input: server.stdout });
const ready = [];
for await (const line of lines) {
  ready.push(line.trim());
  if (ready.length === 2) break;
}
if (ready.length !== 2) throw new Error('studio server did not start');
const [address, token] = ready;

async function rpc(id, method, params) {
  const response = await fetch(`http://${address}/rpc`, {
    method: 'POST',
    headers: {
      authorization: `Bearer ${token}`,
      'content-type': 'application/json',
    },
    body: JSON.stringify({ jsonrpc: '2.0', id, method, params }),
  });
  return response.json();
}

try {
  const plan = JSON.parse(await readFile(join(fixture, 'ai-plan.json'), 'utf8'));
  const first = await rpc(1, 'document.apply', plan.transactions[0]);
  if (first.result?.revision !== 1) throw new Error('ground transaction failed');
  const rejected = await rpc(2, 'document.apply', plan.transactions[1]);
  if (rejected.error?.code !== -32010) throw new Error('invalid transaction was accepted');
  const committed = await rpc(3, 'document.apply', plan.transactions[2]);
  if (committed.result?.revision !== 2) throw new Error('process transaction failed');
  const exported = await rpc(4, 'export.sourceGlb', {});
  if (!exported.result?.sourcePath) throw new Error('GLB export failed');
  console.log(JSON.stringify({
    rejectedOperations: 1,
    finalRevision: committed.result.revision,
    sourcePath: exported.result.sourcePath,
  }, null, 2));
} finally {
  server.kill('SIGTERM');
  await rm(workspace, { recursive: true, force: true });
}
