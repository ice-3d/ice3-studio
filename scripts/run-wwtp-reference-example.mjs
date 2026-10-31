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
const materialPalette = Object.freeze({
  aerobic: { id: 'material-910001', name: 'Aerobic Process', color: [0.16, 0.62, 0.7, 1] },
  anoxic: { id: 'material-910002', name: 'Anoxic Process', color: [0.28, 0.48, 0.68, 1] },
  anaerobic: { id: 'material-910003', name: 'Anaerobic Process', color: [0.62, 0.48, 0.28, 1] },
  intake: { id: 'material-910004', name: 'Intake Structures', color: [0.38, 0.57, 0.62, 1] },
  primary: { id: 'material-910005', name: 'Primary Clarifiers', color: [0.72, 0.62, 0.46, 1] },
  secondary: { id: 'material-910006', name: 'Secondary Clarifiers', color: [0.38, 0.72, 0.8, 1] },
  coagulant: { id: 'material-910007', name: 'Coagulation Structures', color: [0.42, 0.6, 0.74, 1] },
  filter: { id: 'material-910008', name: 'Filtration Structures', color: [0.34, 0.68, 0.64, 1] },
  disinfection: { id: 'material-910009', name: 'Disinfection Structures', color: [0.48, 0.48, 0.76, 1] },
  digester: { id: 'material-910010', name: 'Digesters', color: [0.48, 0.52, 0.4, 1] },
  solids: { id: 'material-910011', name: 'Solids Handling', color: [0.62, 0.48, 0.33, 1] },
  pump: { id: 'material-910012', name: 'Pump Equipment', color: [0.72, 0.42, 0.24, 1] },
  blower: { id: 'material-910013', name: 'Blower Equipment', color: [0.78, 0.66, 0.24, 1] },
  chemical: { id: 'material-910014', name: 'Chemical Dosing', color: [0.42, 0.64, 0.26, 1] },
  instrument: { id: 'material-910015', name: 'Instrumentation', color: [0.38, 0.68, 0.56, 1] },
  utilities: { id: 'material-910016', name: 'Utilities', color: [0.52, 0.6, 0.66, 1] },
  discharge: { id: 'material-910017', name: 'Discharge Structures', color: [0.34, 0.66, 0.72, 1] },
  default: { id: 'material-910018', name: 'Site Equipment', color: [0.6, 0.65, 0.68, 1] },
});

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
await commitOperations(
  'High-fidelity process palette',
  Object.values(materialPalette).map(({ id, name, color }) => ({
    type: 'material.upsert',
    material_id: id,
    name,
    base_color: color,
    opacity: 1,
    edge_color: [color[0] * 0.52, color[1] * 0.52, color[2] * 0.52, 1],
  })),
  0,
  `wwtp-reference-palette-${projectId}`,
);
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
  material_id: equipmentPalette(entry).id,
  metadata: {
    area: String(entry.area ?? ''),
    sourceTag: String(entry.tag ?? ''),
  },
}));
await commitOperations(
  'Create semantic equipment bindings',
  bindings,
  1,
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

function equipmentPalette(entry) {
  const tag = String(entry.tag ?? '');
  if (/^(AE|AT|AX)-/.test(tag)) {
    if (tag.startsWith('AT-')) return materialPalette.anaerobic;
    if (tag.startsWith('AX-')) return materialPalette.anoxic;
    return materialPalette.aerobic;
  }
  if (/^(PC|DW|EQ)-/.test(tag)) return materialPalette.primary;
  if (tag.startsWith('SC-')) return materialPalette.secondary;
  if (/^(CO|CL|MOV-301|MOV-401|MOV-402|MOV-403|MOV-404)-?/.test(tag)) {
    return materialPalette.coagulant;
  }
  if (/^(FL|GAC|UF|P-BW|P-TF)-/.test(tag)) return materialPalette.filter;
  if (/^(DT|OZ|UV|V-OUT|OUT)-/.test(tag)) {
    return materialPalette.discharge;
  }
  if (/^(DIG|FLARE)-/.test(tag)) return materialPalette.digester;
  if (/^(CONV|DRY|DWG|P-SL|SILO|ST)-/.test(tag)) {
    return materialPalette.solids;
  }
  if (tag.startsWith('P-')) return materialPalette.pump;
  if (/^(BLW|FAN)-/.test(tag)) return materialPalette.blower;
  if (/^(ALKALI|BISULFITE|CARBON|FECL3|NACLO|PAC|PAM|POLY|MOV-CHEM)-/.test(tag)) {
    return materialPalette.chemical;
  }
  if (/^(AIT|FIT)-/.test(tag)) return materialPalette.instrument;
  if (/^(GR|GC|GW|IN|CV)-/.test(tag)) return materialPalette.intake;
  if (/^(ODR|FAN|P-UTIL)-/.test(tag)) return materialPalette.utilities;
  return materialPalette.default;
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
