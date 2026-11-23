import { spawn } from 'node:child_process';
import { cp, mkdir, mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { startAgentGateway } from './agent-gateway-fixture.mjs';

const repositoryRoot = process.cwd();
const workspace = await mkdtemp(join(tmpdir(), 'ice3-studio-viewer-e2e-'));
const fixture = join(workspace, 'fixtures/wastewater-mini');

await mkdir(join(workspace, 'fixtures'), { recursive: true });
await cp(
  join(repositoryRoot, 'fixtures/wastewater-mini'),
  fixture,
  { recursive: true },
);

let cleanedUp = false;
let gateway;
let server;
async function cleanup() {
  if (cleanedUp) return;
  cleanedUp = true;
  gateway?.close();
  server?.kill('SIGTERM');
  await rm(workspace, { recursive: true, force: true });
}

gateway = await startAgentGateway({
  studioUrl: 'http://127.0.0.1:8120',
  token: 'studio-e2e',
  projectId: 'wastewater-mini',
});

server = spawn(
  'cargo',
  [
    'run',
    '-q',
    '-p',
    'studio-cli',
    '--bin',
    'studio-cli',
    '--',
    'serve',
    '--workspace',
    workspace,
    '--project',
    'wastewater-mini',
    '--fixture',
    fixture,
    '--write-mode',
    'autonomous',
  ],
  {
    cwd: repositoryRoot,
    env: {
      ...process.env,
      STUDIO_SESSION_TOKEN: 'studio-e2e',
      STUDIO_VIEWER_DIR: join(repositoryRoot, 'apps/viewer/dist'),
      STUDIO_AGENT_GATEWAY_URL: gateway.url,
    },
    stdio: 'inherit',
  },
);

server.once('error', async (error) => {
  console.error(error);
  await cleanup();
  process.exit(1);
});
server.once('exit', async (code, signal) => {
  await cleanup();
  process.exit(signal ? 1 : (code ?? 1));
});

for (const signal of ['SIGINT', 'SIGTERM']) {
  process.once(signal, () => {
    cleanup().finally(() => process.exit(1));
  });
}
