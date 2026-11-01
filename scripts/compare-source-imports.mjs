import { createHash } from 'node:crypto';
import { mkdtemp, readFile, rm, writeFile, mkdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const studioRoot = resolve(scriptDir, '..');
const fixture = resolve(
  studioRoot,
  '../ice3-toolchain/tests/fixtures/obj/synthetic-plant.obj',
);
const baselinePath = join(studioRoot, 'fixtures/parity/obj-baseline-v1.json');
const reportPath = join(studioRoot, 'output/source-import-parity.json');
const temporaryRoot = await mkdtemp(join(tmpdir(), 'ice3-import-parity-'));
const output = join(temporaryRoot, 'obj.native.glb');

try {
  const baseline = JSON.parse(await readFile(baselinePath, 'utf8'));
  const sourceBytes = await readFile(fixture);
  const sourceDigest = sha256(sourceBytes);
  const result = run('cargo', [
    'run',
    '-q',
    '-p',
    'studio-cli',
    '--',
    'import',
    fixture,
    '--output',
    output,
    '--asset-id',
    'parity-obj',
    '--adapter',
    'obj',
  ], studioRoot);
  const command = JSON.parse(result.stdout);
  const nativeBytes = await readFile(output);
  const parsed = parseGlb(nativeBytes);
  const metrics = metricsFor(parsed);
  const comparisons = {
    sourceDigest: {
      baseline: baseline.sourceDigest,
      native: sourceDigest,
      equal: baseline.sourceDigest === sourceDigest,
    },
    stats: {
      baseline: baseline.stats,
      native: metrics.stats,
      equal: (
        baseline.stats.nodeCount === metrics.stats.nodeCount
        && baseline.stats.meshCount === metrics.stats.meshCount
        && baseline.stats.triangleCount === metrics.stats.triangleCount
        && baseline.stats.semanticMaterialCount === metrics.stats.semanticMaterialCount
      ),
    },
    selectionIds: {
      baseline: baseline.selectionIds,
      native: metrics.selectionIds,
      equal: JSON.stringify(baseline.selectionIds) === JSON.stringify(metrics.selectionIds),
    },
    bounds: {
      baseline: baseline.bounds,
      native: metrics.bounds,
      equal: arraysClose(baseline.bounds.min, metrics.bounds.min)
        && arraysClose(baseline.bounds.max, metrics.bounds.max),
    },
    materials: {
      baseline: baseline.materials,
      native: metrics.materials,
      equal: materialsClose(baseline.materials, metrics.materials),
    },
    warningCodes: {
      baseline: baseline.warningCodes,
      native: command.warnings.map((warning) => warning.code).sort(),
      equal: JSON.stringify(baseline.warningCodes)
        === JSON.stringify(command.warnings.map((warning) => warning.code).sort()),
    },
    vertexCount: {
      baseline: baseline.stats.vertexCount,
      native: metrics.stats.vertexCount,
      equal: false,
      explanation: 'The legacy adapter split vertices for prepared edges; the native source writer preserves welded OBJ vertices.',
    },
    edgeSegmentCount: {
      baseline: baseline.stats.edgeSegmentCount,
      native: metrics.stats.edgeSegmentCount,
      equal: false,
      explanation: 'Legacy generated render edges are not part of the native source import; consumers derive render emphasis from materials.',
    },
    byteDigest: {
      baseline: baseline.baselineGlbDigest,
      native: sha256(nativeBytes),
      equal: false,
      explanation: 'Vertex preparation and deterministic JSON layout differ while semantic counts, bounds, materials and Selection IDs match.',
    },
  };
  const unexplained = Object.entries(comparisons)
    .filter(([key, value]) => (
      !['vertexCount', 'edgeSegmentCount', 'byteDigest'].includes(key)
      && value.equal !== true
    ))
    .map(([key]) => key);
  const report = {
    kind: 'ice3-source-import-parity',
    schemaVersion: '1.0',
    status: unexplained.length === 0 ? 'pass' : 'fail',
    fixture: relative(studioRoot, fixture),
    adapter: 'obj',
    failures: unexplained,
    comparisons,
  };
  await mkdir(dirname(reportPath), { recursive: true });
  await writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`);
  console.log(JSON.stringify(report, null, 2));
  if (unexplained.length > 0) process.exitCode = 1;
} finally {
  await rm(temporaryRoot, { recursive: true, force: true });
}

function metricsFor(glb) {
  let triangleCount = 0;
  let vertexCount = 0;
  let edgeSegmentCount = 0;
  const usedMaterials = new Set();
  for (const mesh of glb.json.meshes ?? []) {
    for (const primitive of mesh.primitives ?? []) {
      const indexCount = glb.json.accessors[primitive.indices]?.count ?? 0;
      if ((primitive.mode ?? 4) === 4) {
        triangleCount += indexCount / 3;
        vertexCount += glb.json.accessors[primitive.attributes.POSITION]?.count ?? 0;
      } else if (primitive.mode === 1) {
        edgeSegmentCount += indexCount / 2;
      }
      if (primitive.material !== undefined) usedMaterials.add(primitive.material);
    }
  }
  const materials = [...usedMaterials]
    .map((index) => glb.json.materials[index])
    .filter((material) => !String(material.name).endsWith(' edges'))
    .map((material) => ({
      name: material.name,
      baseColorFactor: material.pbrMetallicRoughness.baseColorFactor,
    }))
    .sort((left, right) => left.name.localeCompare(right.name, 'en'));
  return {
    stats: {
      nodeCount: glb.json.nodes?.length ?? 0,
      meshCount: glb.json.meshes?.length ?? 0,
      triangleCount,
      vertexCount,
      edgeSegmentCount,
      semanticMaterialCount: materials.length,
    },
    selectionIds: (glb.json.nodes ?? [])
      .map((node) => node.extras?.ice3?.selectionId)
      .filter(Boolean)
      .sort(),
    bounds: glb.json.asset.extras.ice3.bounds,
    materials,
  };
}

function parseGlb(bytes) {
  if (bytes.readUInt32LE(0) !== 0x46546c67) throw new Error('not a GLB');
  let offset = 12;
  let json;
  while (offset < bytes.length) {
    const length = bytes.readUInt32LE(offset);
    const kind = bytes.readUInt32LE(offset + 4);
    if (kind === 0x4e4f534a) {
      json = JSON.parse(bytes.subarray(offset + 8, offset + 8 + length).toString('utf8').trim());
    }
    offset += 8 + length;
  }
  if (!json) throw new Error('GLB JSON chunk is missing');
  return { json };
}

function arraysClose(left, right) {
  return left.length === right.length
    && left.every((value, index) => Math.abs(value - right[index]) <= 1e-5);
}

function materialsClose(left, right) {
  return left.length === right.length
    && left.every((material, index) => (
      material.name === right[index].name
      && arraysClose(material.baseColorFactor, right[index].baseColorFactor)
    ));
}

function sha256(value) {
  return createHash('sha256').update(value).digest('hex');
}

function run(command, args, cwd) {
  const result = spawnSync(command, args, {
    cwd,
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
  });
  if (result.status !== 0) {
    throw new Error(`${command} failed:\n${result.stderr}\n${result.stdout}`);
  }
  return result;
}
