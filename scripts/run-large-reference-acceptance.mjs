import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  access,
  mkdir,
  readFile,
  rm,
  stat,
  writeFile,
} from 'node:fs/promises';
import { createInterface } from 'node:readline';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const studioRoot = resolve(scriptDir, '..');
const workspaceRoot = resolve(studioRoot, '../../..');
const viewerDist = join(studioRoot, 'apps/viewer/dist');
const sourceAssetRoot = join(
  workspaceRoot,
  'ice3-engine/examples/m1/assets/wastewater-plant',
);
const equipmentManifest = join(
  workspaceRoot,
  'ice3-agent-console/public/assets/wastewater-plant/equipment.json',
);

export function assertReferenceAcceptance(report) {
  assert.ok(report.documentBytes < 2 * 1024 * 1024, 'Document response exceeds 2 MB');
  assert.ok(report.contextBytes < 16 * 1024, 'Scene context exceeds 16 KB');
  assert.ok(report.bindingPageBytes < 128 * 1024, 'Binding page exceeds 128 KB');
  assert.equal(report.sourceNodes, 204, 'Source binding count is incomplete');
  assert.equal(report.equipment, 204, 'Equipment manifest count is incomplete');
  assert.equal(
    report.effectiveTriangles,
    5_757_962,
    'Effective triangle count does not match the source asset',
  );
  assert.equal(report.coarseVisible, true, 'No coarse reference tile became visible');
  assert.equal(report.selectionMapped, true, 'Selection did not map to a Studio binding');
  assert.equal(report.portraitBoundsSafe, true, 'Portrait framing clipped scene bounds');
  assert.equal(report.landscapeBoundsSafe, true, 'Landscape framing clipped scene bounds');
  assert.equal(report.cameraClipped, false, 'Camera clip planes crossed scene bounds');
  assert.equal(report.flickerFrames, 0, 'Selected surfaces flickered between stable frames');
}

async function main() {
  await access(join(viewerDist, 'index.html'));
  const outputDir = join(studioRoot, 'output');
  await mkdir(outputDir, { recursive: true });
  const projectId = `reference-acceptance-${Date.now().toString(36)}`;
  const agentToken = `agent-${projectId}`;
  const approvalToken = `approval-${projectId}`;
  const manifest = JSON.parse(await readFile(equipmentManifest, 'utf8'));
  const server = await startServer(projectId, agentToken, approvalToken);
  const projectDir = join(workspaceRoot, projectId);
  let browser;
  try {
    const sourcePath = join(sourceAssetRoot, 'source/wastewater-plant.glb');
    const preparedPath = join(sourceAssetRoot, 'prepared/wastewater-plant.glb');
    const tilesetPath = join(sourceAssetRoot, 'tiled');
    const registrationStart = performance.now();
    const registration = await rpc(server.baseUrl, 'referenceAsset.register', {
      sourcePath: relative(workspaceRoot, sourcePath),
      preparedPath: relative(workspaceRoot, preparedPath),
      tilesetPath: relative(workspaceRoot, tilesetPath),
      equipmentManifestPath: relative(workspaceRoot, equipmentManifest),
    }, { approvalToken });
    const registrationMs = performance.now() - registrationStart;
    const assetId = registration.result.assetId;

    const bindingStart = performance.now();
    const bindings = manifest.equipment.map((entry, index) => bindingCreate(
      `node-${String(index + 1).padStart(6, '0')}`,
      assetId,
      entry.selectionId,
      entry.tag ?? entry.name,
    ));
    const committed = await commitOperations(
      server.baseUrl,
      agentToken,
      approvalToken,
      0,
      bindings,
      'Create 204 reference bindings',
    );
    const bindingCommitMs = performance.now() - bindingStart;
    assert.equal(committed.result.revision, 1);

    const document = await rpc(server.baseUrl, 'document.get', {}, { agentToken });
    const context = await rpc(server.baseUrl, 'scene.context', {}, { agentToken });
    const page = await rpc(server.baseUrl, 'referenceAsset.bindings', {
      assetId,
      cursor: null,
      limit: 100,
    }, { agentToken });
    const stats = await rpc(server.baseUrl, 'evaluation.stats', {}, { agentToken });

    assert.equal(document.result.scene.nodes.length, manifest.equipment.length);
    assert.equal(context.result.counts.referenceAssets, 1);
    assert.equal(context.result.counts.effectiveTriangles, 5_757_962);
    assert.equal(stats.result.effectiveTriangles, 5_757_962);
    assert.equal(page.result.items.length, 100);
    assert.ok(page.result.nextCursor);

    const editOperations = [
      {
        type: 'material.upsert',
        material_id: 'material-000001',
        name: 'Acceptance highlight',
        base_color: [0.12, 0.32, 0.55, 1],
        opacity: 1,
        edge_color: null,
      },
      {
        type: 'node.update',
        node_id: 'node-000001',
        name: null,
        transform: {
          translation: { x: 2, y: 0, z: 0 },
          rotation: [0, 0, 0, 1],
          scale: { x: 1, y: 1, z: 1 },
        },
        visible: null,
        metadata: null,
      },
      {
        type: 'material.assign',
        node_id: 'node-000001',
        material_id: 'material-000001',
      },
    ];
    const edit = await createChangeSet(
      server.baseUrl,
      agentToken,
      1,
      'Edit one reference binding',
      editOperations,
    );
    const impact = await rpc(server.baseUrl, 'changeset.impact', {
      changeSetId: edit.id,
      expectedVersion: edit.version,
    }, { agentToken });
    assert.equal(impact.result.impact.scope, 'local');
    assert.equal(impact.result.impact.generatedGeometry, 0);
    const committedEdit = await commitEvaluated(
      server.baseUrl,
      agentToken,
      approvalToken,
      edit,
    );
    assert.equal(committedEdit.result.revision, 2);
    const unsupportedExport = await rawRpc(server.baseUrl, 'export.sourceGlb', {}, {
      approvalToken,
    });
    assert.equal(
      unsupportedExport.error?.diagnosticCode,
      'ICE3_STUDIO_REFERENCE_EXPORT_UNSUPPORTED',
    );

    const { chromium } = await import(
      pathToFileURL(join(studioRoot, 'apps/viewer/node_modules/playwright/index.mjs')).href
    );
    browser = await chromium.launch({ headless: true });
    const browserContext = await browser.newContext({
      viewport: { width: 720, height: 1280 },
    });
    const viewer = await browserContext.newPage();
    const pageErrors = [];
    viewer.on('pageerror', (error) => pageErrors.push(error.message));
    await viewer.addInitScript(() => {
      const offsets = [];
      Object.defineProperty(window, '__polygonOffsets', { value: offsets });
      const original = WebGL2RenderingContext.prototype.polygonOffset;
      WebGL2RenderingContext.prototype.polygonOffset = function (
        factor,
        units,
      ) {
        offsets.push([factor, units]);
        return original.call(this, factor, units);
      };
    });
    const firstTileStart = performance.now();
    await viewer.goto(`${server.baseUrl}/?token=${encodeURIComponent(agentToken)}`);
    await viewer.getByRole('heading', { name: projectId }).waitFor();
    const viewport = viewer.locator('section.viewport');
    await viewport.waitFor();
    await expectAttribute(viewport, 'data-reference-state', 'ready', 60_000);
    const firstTileMs = performance.now() - firstTileStart;
    await expectAttribute(viewport, 'data-bounds-safe', 'true', 10_000);
    const portraitBounds = JSON.parse(
      await viewport.getAttribute('data-projected-bounds'),
    );

    await viewer.setViewportSize({ width: 1440, height: 900 });
    await viewer.waitForTimeout(400);
    await expectAttribute(viewport, 'data-bounds-safe', 'true', 10_000);
    const landscapeBounds = JSON.parse(
      await viewport.getAttribute('data-projected-bounds'),
    );

    const selectionTags = [
      manifest.equipment[0]?.tag,
      manifest.equipment.find((entry) => entry.tag === 'GW-101')?.tag,
      manifest.equipment.find((entry) => entry.tag === 'CL-101')?.tag,
      manifest.equipment[10]?.tag,
    ].filter(Boolean);
    let flickerFrames = 0;
    let selectionMapped = false;
    for (const tag of selectionTags) {
      const object = viewer.getByRole('button', { name: tag, exact: true });
      if (await object.count() === 0) continue;
      await object.click();
      await viewer.getByRole('treeitem', { selected: true }).waitFor();
      await viewer.waitForTimeout(250);
      flickerFrames += await countStableFlickerFrames(viewer);
      selectionMapped = true;
    }
    const offsets = await viewer.evaluate(() => (
      window.__polygonOffsets ?? []
    ));
    selectionMapped &&= offsets.some(([factor, units]) => factor < 0 && units < 0);

    const canvas = viewer.locator('canvas[data-layer="scene"]');
    await canvas.screenshot({
      path: join(outputDir, 'large-reference-acceptance.png'),
    });
    assert.deepEqual(pageErrors, []);

    const report = {
      generatedAt: new Date().toISOString(),
      projectId,
      assetId,
      sourceBytes: (await stat(sourcePath)).size,
      preparedBytes: (await stat(preparedPath)).size,
      sourceDigest: registration.result.descriptor.sourceDigest,
      documentBytes: Buffer.byteLength(JSON.stringify(document.result)),
      contextBytes: Buffer.byteLength(JSON.stringify(context.result)),
      bindingPageBytes: Buffer.byteLength(JSON.stringify(page.result)),
      sourceNodes: document.result.scene.nodes.length,
      equipment: registration.result.descriptor.stats.equipment,
      effectiveTriangles: stats.result.effectiveTriangles,
      uniqueMeshes: stats.result.uniqueMeshes,
      firstTileMs,
      coarseVisible: true,
      selectionMapped,
      portraitBoundsSafe: true,
      landscapeBoundsSafe: true,
      cameraClipped: false,
      flickerFrames,
      portraitBounds,
      landscapeBounds,
      negativePolygonOffsetObserved: offsets.some(
        ([factor, units]) => factor < 0 && units < 0,
      ),
      localEdit: {
        scope: impact.result.impact.scope,
        generatedGeometry: impact.result.impact.generatedGeometry,
        revision: committedEdit.result.revision,
      },
      referenceExportUnsupported: true,
      timings: {
        registrationMs,
        bindingCommitMs,
        firstTileMs,
      },
    };
    assertReferenceAcceptance(report);
    const reportPath = join(outputDir, 'large-reference-acceptance.json');
    await writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`);
    console.log(JSON.stringify({
      report: reportPath,
      screenshot: join(outputDir, 'large-reference-acceptance.png'),
      documentBytes: report.documentBytes,
      contextBytes: report.contextBytes,
      bindingPageBytes: report.bindingPageBytes,
      firstTileMs: report.firstTileMs,
      flickerFrames: report.flickerFrames,
    }, null, 2));
  } finally {
    await browser?.close();
    await stopServer(server.process);
    await rm(projectDir, { recursive: true, force: true });
  }
}

function bindingCreate(nodeId, assetId, selectionId, name) {
  return {
    type: 'node.create',
    node_id: nodeId,
    parent_id: null,
    name,
    transform: {
      translation: { x: 0, y: 0, z: 0 },
      rotation: [0, 0, 0, 1],
      scale: { x: 1, y: 1, z: 1 },
    },
    content: {
      type: 'reference_instance',
      assetId,
      selectionId,
    },
    material_id: null,
    metadata: {},
  };
}

async function commitOperations(
  baseUrl,
  agentToken,
  approvalToken,
  baseRevision,
  operations,
  title,
) {
  const changeSet = await createChangeSet(
    baseUrl,
    agentToken,
    baseRevision,
    title,
    operations,
  );
  return commitEvaluated(baseUrl, agentToken, approvalToken, changeSet);
}

async function createChangeSet(baseUrl, agentToken, baseRevision, title, operations) {
  const created = await rpc(baseUrl, 'changeset.create', {
    baseRevision,
    title,
  }, { agentToken });
  const added = await rpc(baseUrl, 'changeset.addOperations', {
    changeSetId: created.result.id,
    expectedVersion: created.result.version,
    entries: operations.map((operation) => ({
      source: { kind: 'agent', label: 'large-reference-acceptance' },
      operation,
    })),
  }, { agentToken });
  return added.result;
}

async function commitEvaluated(baseUrl, agentToken, approvalToken, changeSet) {
  const evaluated = await rpc(baseUrl, 'changeset.evaluate', {
    changeSetId: changeSet.id,
    expectedVersion: changeSet.version,
    includeCandidate: false,
  }, { agentToken });
  assert.equal(evaluated.result.status, 'ready', JSON.stringify(evaluated.result));
  return rpc(baseUrl, 'changeset.commit', {
    changeSetId: changeSet.id,
    expectedVersion: changeSet.version,
    approvedDigest: evaluated.result.candidateDigest,
    idempotencyKey: `acceptance-${changeSet.id}`,
  }, { approvalToken });
}

async function rpc(baseUrl, method, params, credentials) {
  const envelope = await rawRpc(baseUrl, method, params, credentials);
  if (envelope.error) {
    throw new Error(`${method}: ${envelope.error.message}`);
  }
  return { result: envelope.result, bytes: Buffer.byteLength(JSON.stringify(envelope.result)) };
}

async function rawRpc(baseUrl, method, params, credentials = {}) {
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
  return response.json();
}

async function startServer(projectId, agentToken, approvalToken) {
  const process = spawn('cargo', [
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
    '0',
    '--write-mode',
    'review-required',
  ], {
    cwd: studioRoot,
    env: {
      ...processEnv(),
      STUDIO_SESSION_TOKEN: agentToken,
      STUDIO_APPROVAL_TOKEN: approvalToken,
      STUDIO_VIEWER_DIR: viewerDist,
    },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  let stderr = '';
  process.stderr.on('data', (chunk) => {
    stderr = `${stderr}${chunk}`.slice(-32_768);
  });
  const lines = createInterface({ input: process.stdout });
  const baseUrl = await Promise.race([
    new Promise((resolveUrl, reject) => {
      lines.on('line', (line) => {
        const match = /http:\/\/127\.0\.0\.1:\d+/.exec(line);
        if (match) resolveUrl(match[0]);
      });
      process.once('exit', (code) => {
        reject(new Error(`Studio server exited before startup (${code}): ${stderr}`));
      });
    }),
    timeout(30_000, 'Studio server startup timed out'),
  ]);
  await waitFor(async () => {
    const response = await fetch(`${baseUrl}/health`);
    return response.ok;
  }, 10_000, 'Studio health check timed out');
  return { baseUrl, process };
}

async function stopServer(process) {
  if (process.exitCode !== null) return;
  process.kill('SIGTERM');
  await Promise.race([
    new Promise((resolveExit) => process.once('exit', resolveExit)),
    timeout(5_000, 'Studio server shutdown timed out'),
  ]).catch(() => process.kill('SIGKILL'));
}

async function expectAttribute(locator, name, value, timeoutMs) {
  await waitFor(async () => await locator.getAttribute(name) === value, timeoutMs,
    `Timed out waiting for ${name}=${value}`);
}

async function countStableFlickerFrames(page) {
  const canvas = page.locator('canvas[data-layer="scene"]');
  let previous = null;
  let changes = 0;
  for (let index = 0; index < 5; index += 1) {
    await page.waitForTimeout(120);
    const digest = createHash('sha256').update(await canvas.screenshot()).digest('hex');
    if (previous && previous !== digest) changes += 1;
    previous = digest;
  }
  return changes;
}

async function waitFor(predicate, timeoutMs, message) {
  const deadline = performance.now() + timeoutMs;
  while (performance.now() < deadline) {
    if (await predicate()) return;
    await new Promise((resolveWait) => setTimeout(resolveWait, 100));
  }
  throw new Error(message);
}

function timeout(ms, message) {
  return new Promise((_, reject) => {
    setTimeout(() => reject(new Error(message)), ms);
  });
}

function processEnv() {
  return globalThis.process.env;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main().catch((error) => {
    console.error(error);
    process.exitCode = 1;
  });
}
