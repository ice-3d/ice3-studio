import { spawn } from 'node:child_process';
import { access, mkdir, readFile, writeFile } from 'node:fs/promises';
import { createInterface } from 'node:readline';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const studioRoot = resolve(scriptDir, '..');
const workspaceRoot = dirname(studioRoot);
const projectId = process.env.ICE3_WWTP_PROJECT_ID
  ?? `wwtp-reference-studio-${Date.now().toString(36)}`;
const agentToken = process.env.ICE3_WWTP_AGENT_TOKEN ?? `agent-${projectId}`;
const approvalToken = process.env.ICE3_WWTP_APPROVAL_TOKEN ?? `approval-${projectId}`;
const port = Number(process.env.ICE3_WWTP_PORT ?? 8149);
const viewerDist = join(studioRoot, 'apps/viewer/dist');
const sourceRoot = join(
  workspaceRoot,
  'ice3-engine/examples/m1/assets/wastewater-plant',
);
const equipmentManifest = join(
  workspaceRoot,
  'ice3-agent-console/public/assets/wastewater-plant/equipment.json',
);
await access(join(viewerDist, 'index.html'));
const processHandle = spawn('cargo', [
  'run',
  '-q',
  '-p',
  'studio-cli',
  '--',
  'serve',
  '--workspace',
  workspaceRoot,
  '--project',
  projectId,
  '--port',
  String(port),
  '--write-mode',
  'review-required',
], {
  cwd: studioRoot,
  env: {
    ...process.env,
    STUDIO_SESSION_TOKEN: agentToken,
    STUDIO_APPROVAL_TOKEN: approvalToken,
    STUDIO_VIEWER_DIR: viewerDist,
  },
  stdio: ['ignore', 'pipe', 'pipe'],
});

let stderr = '';
processHandle.stderr.on('data', (chunk) => {
  stderr = `${stderr}${chunk}`.slice(-32_768);
});
processHandle.once('exit', (code) => {
  if (code && code !== 0) console.error(stderr);
});

const baseUrl = await waitForStartup(processHandle);
await waitForHealth(baseUrl);

const sourcePath = join(sourceRoot, 'source/wastewater-plant.glb');
const preparedPath = join(sourceRoot, 'prepared/wastewater-plant.glb');
const tilesetPath = join(sourceRoot, 'tiled');
const registration = await rpc('referenceAsset.register', {
  sourcePath: relative(workspaceRoot, sourcePath),
  preparedPath: relative(workspaceRoot, preparedPath),
  tilesetPath: relative(workspaceRoot, tilesetPath),
  equipmentManifestPath: relative(workspaceRoot, equipmentManifest),
}, { approvalToken });

const manifest = JSON.parse(await readFile(equipmentManifest, 'utf8'));
const bindings = manifest.equipment.map((entry, index) => ({
  type: 'node.create',
  node_id: `node-${String(index + 1).padStart(6, '0')}`,
  parent_id: null,
  name: entry.tag ?? entry.name,
  transform: {
    translation: { x: 0, y: 0, z: 0 },
    rotation: [0, 0, 0, 1],
    scale: { x: 1, y: 1, z: 1 },
  },
  content: {
    type: 'reference_instance',
    assetId: registration.assetId,
    selectionId: entry.selectionId,
  },
  material_id: null,
  metadata: {
    area: String(entry.area ?? ''),
    sourceTag: String(entry.tag ?? ''),
  },
}));
await commitOperations(
  'Create semantic equipment bindings',
  bindings,
  0,
  `wwtp-reference-bindings-${projectId}`,
);

const stats = await rpc('evaluation.stats', {}, { agentToken });
const context = await rpc('scene.context', {}, { agentToken });
const url = `${baseUrl}/?token=${encodeURIComponent(agentToken)}&reference=glb`;
const report = {
  projectId,
  assetId: registration.assetId,
  url,
  sourceNodes: registration.descriptor.stats.sourceNodes,
  equipment: registration.descriptor.stats.equipment,
  materials: registration.descriptor.stats.materials,
  effectiveTriangles: stats.effectiveTriangles,
  uniqueMeshes: stats.uniqueMeshes,
  referenceAssets: context.referenceAssets,
};
await mkdir(join(studioRoot, 'output'), { recursive: true });
await writeFile(
  join(studioRoot, 'output/wwtp-reference-example.json'),
  `${JSON.stringify(report, null, 2)}\n`,
);
console.log(JSON.stringify(report, null, 2));

async function rpc(method, params = {}, credentials = {}) {
  const headers = { 'content-type': 'application/json' };
  if (credentials.agentToken) {
    headers.authorization = `Bearer ${credentials.agentToken}`;
  }
  if (credentials.approvalToken) {
    headers.cookie = `ice3_studio_approval=${credentials.approvalToken}`;
  }
  const response = await fetch(`${baseUrl}/rpc`, {
    method: 'POST',
    headers,
    body: JSON.stringify({
      jsonrpc: '2.0',
      id: `${method}-${Date.now()}`,
      method,
      params,
    }),
  });
  const envelope = await response.json();
  if (envelope.error) throw new Error(`${method}: ${envelope.error.message}`);
  return envelope.result;
}

async function commitOperations(title, operations, baseRevision, idempotencyKey) {
  const created = await rpc('changeset.create', {
    baseRevision,
    title,
  }, { agentToken });
  const added = await rpc('changeset.addOperations', {
    changeSetId: created.id,
    expectedVersion: created.version,
    entries: operations.map((operation) => ({
      source: { kind: 'agent', label: 'wwtp-reference-example' },
      operation,
    })),
  }, { agentToken });
  const evaluated = await rpc('changeset.evaluate', {
    changeSetId: created.id,
    expectedVersion: added.version,
    includeCandidate: false,
  }, { agentToken });
  if (evaluated.status !== 'ready' || !evaluated.candidateDigest) {
    throw new Error(`${title} is not ready: ${JSON.stringify(evaluated.diagnostics)}`);
  }
  return rpc('changeset.commit', {
    changeSetId: created.id,
    expectedVersion: evaluated.version,
    approvedDigest: evaluated.candidateDigest,
    idempotencyKey,
  }, { approvalToken });
}


async function waitForStartup(child) {
  const lines = createInterface({ input: child.stdout });
  const startup = [];
  for await (const line of lines) {
    if (line.trim()) startup.push(line.trim());
    if (startup.length === 2) break;
  }
  if (startup.length !== 2) throw new Error('Studio server did not start');
  return startup[0].startsWith('http') ? startup[0] : `http://${startup[0]}`;
}

async function waitForHealth(url) {
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(`${url}/health`);
      if (response.ok) return;
    } catch {
      // The server prints its bound port before the listener is fully ready.
    }
    await new Promise((resolveWait) => setTimeout(resolveWait, 100));
  }
  throw new Error('Studio health check timed out');
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  console.log(`WWTP reference example: ${url}`);
}
