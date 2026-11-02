import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rm,
  writeFile,
} from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const studioRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const reportPath = resolve(studioRoot, 'output/partition-parity.json');

if (
  process.argv[1]
  && resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  const report = await runParity().catch((error) => ({
    kind: 'ice3-partition-parity',
    schemaVersion: '1.0',
    status: 'fail',
    failures: ['execution'],
    error: error instanceof Error ? error.message : String(error),
    cases: [],
  }));
  await mkdir(dirname(reportPath), { recursive: true });
  await writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`);
  console.log(JSON.stringify(report, null, 2));
  if (report.status !== 'pass') process.exitCode = 1;
}

async function runParity() {
  const studioBinary = resolveStudioBinary();
  const temporary = await mkdtemp(join(tmpdir(), 'ice3-partition-parity-'));
  try {
    const source = join(temporary, 'partition-parity.obj');
    await writeFile(source, gridObj(4, 8));
    const baseOptions = {
      maxTriangles: 128,
      maxBytes: 33_554_432,
      lodRatio: 0.5,
      lodMaxTriangles: 512,
      lodMinUnitTriangles: 8,
      lodCurveSamples: 8,
      lodMaxMemoryBytes: 67_108_864,
    };
    const coarseRootBytes = await measureCoarseRoot(
      studioBinary,
      temporary,
      source,
      baseOptions,
    );
    const chunkBytes = Math.max(1, Math.floor(coarseRootBytes * 0.65));
    const definitions = [
      ['direct-no-lod', []],
      ['proportional-lod', [
        '--lod',
        '--lod-allocation',
        'proportional',
      ]],
      ['adaptive-lod', [
        '--lod',
        '--lod-allocation',
        'adaptive',
      ]],
      ['semantic-proxy-auto', [
        '--lod',
        '--lod-allocation',
        'adaptive',
        '--semantic-proxy',
        'auto',
      ]],
      ['static-render-batch', ['--render-batch', 'static']],
      ['chunked-coarse', [
        '--lod',
        '--lod-allocation',
        'adaptive',
        '--coarse-chunk-max-bytes',
        String(chunkBytes),
      ]],
      ['skip-edges', ['--skip-edges']],
    ];

    const cases = [];
    const failures = [];
    for (const [name, extraArgs] of definitions) {
      const first = await runCase(
        studioBinary,
        temporary,
        source,
        baseOptions,
        name,
        'first',
        extraArgs,
      );
      const second = await runCase(
        studioBinary,
        temporary,
        source,
        baseOptions,
        name,
        'second',
        extraArgs,
      );
      const deterministic =
        first.reportDigest === second.reportDigest
        && first.tilesetDigest === second.tilesetDigest
        && first.files.length === second.files.length
        && first.files.every((file, index) => (
          file.path === second.files[index].path
          && file.digest === second.files[index].digest
        ));
      const expectsEdges = name !== 'skip-edges';
      const expectsCoarse = name !== 'direct-no-lod'
        && name !== 'static-render-batch'
        && name !== 'skip-edges';
      const caseFailures = [];
      if (!deterministic) caseFailures.push('non-deterministic output');
      if (expectsEdges && first.edge.segmentCount === 0) {
        caseFailures.push('missing edge segments');
      }
      if (!expectsEdges && first.edge !== null) {
        caseFailures.push('skip-edges emitted edge primitives');
      }
      if (expectsCoarse && first.coarseCount === 0) {
        caseFailures.push('missing coarse output');
      }
      if (name === 'chunked-coarse' && first.coarseCount < 2) {
        caseFailures.push('coarse output was not chunked');
      }
      if (!first.budget.withinBudget) {
        caseFailures.push('byte budget exceeded');
      }
      if (first.uris.length === 0 || first.bounds.length === 0
        || first.selectionIds.length === 0) {
        caseFailures.push('Tileset hierarchy, bounds, or selections are missing');
      }
      failures.push(...caseFailures.map((failure) => `${name}:${failure}`));
      cases.push({
        name,
        status: caseFailures.length === 0 ? 'pass' : 'fail',
        failures: caseFailures,
        deterministic,
        edge: first.edge,
        partCount: first.partCount,
        coarseCount: first.coarseCount,
        renderBatchCount: first.renderBatchCount,
        semanticProxy: name === 'semantic-proxy-auto'
          && first.semanticProxyObserved,
        uris: first.uris,
        bounds: first.bounds,
        selectionIds: first.selectionIds,
        triangleCount: first.triangleCount,
        budget: first.budget,
        files: first.files,
        tilesetDigest: first.tilesetDigest,
      });
    }

    return {
      kind: 'ice3-partition-parity',
      schemaVersion: '1.0',
      status: failures.length === 0 ? 'pass' : 'fail',
      studioBinary: basename(studioBinary),
      fixture: {
        kind: 'generated-obj',
        objects: 4,
        gridSize: 8,
        trianglesPerObject: 128,
      },
      failures,
      cases,
    };
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
}

async function measureCoarseRoot(studioBinary, temporary, source, options) {
  const output = join(temporary, 'coarse-calibration');
  runPartition(
    studioBinary,
    source,
    output,
    options,
    ['--lod', '--lod-allocation', 'adaptive'],
  );
  const files = await listFiles(output);
  const root = files.find((file) => file.path.startsWith('coarse/'));
  if (!root) throw new Error('coarse calibration produced no coarse GLB');
  return root.byteLength;
}

async function runCase(
  studioBinary,
  temporary,
  source,
  options,
  name,
  suffix,
  extraArgs,
) {
  const output = join(temporary, `${name}-${suffix}`);
  const result = runPartition(
    studioBinary,
    source,
    output,
    options,
    extraArgs,
  );
  const files = await listFiles(output);
  const tilesetBytes = await readFile(join(output, 'tileset.json'));
  const tileset = JSON.parse(tilesetBytes);
  const glbFiles = [];
  for (const file of files) {
    if (!file.path.endsWith('.glb')) continue;
    const bytes = await readFile(join(output, file.path));
    const document = parseGlb(bytes);
    glbFiles.push({ ...file, bytes, document });
  }
  const stats = summarizeGeometry(glbFiles);
  const coarseBudget = optionsForCase(options, extraArgs)
    .coarseChunkMaxBytes;
  const withinBudget = glbFiles.every((file) => (
    file.path.startsWith('coarse/')
      ? file.byteLength <= coarseBudget
      : file.byteLength <= options.maxBytes
  ));
  const { output: _output, ...reportIdentity } = result;
  return {
    reportDigest: sha256(Buffer.from(JSON.stringify(reportIdentity))),
    tilesetDigest: sha256(tilesetBytes),
    files: files.map(({ path, byteLength, digest }) => ({
      path,
      byteLength,
      digest,
    })),
    partCount: files.filter((file) => file.path.startsWith('parts/')).length,
    coarseCount: files.filter((file) => file.path.startsWith('coarse/')).length,
    renderBatchCount: result.stats.renderBatchCount,
    semanticProxyObserved: stats.semanticProxyObserved,
    edge: stats.edge,
    triangleCount: stats.triangleCount,
    uris: collectUris(tileset.root),
    bounds: collectBounds(tileset.root),
    selectionIds: stats.selectionIds,
    budget: {
      maxBytes: options.maxBytes,
      coarseChunkMaxBytes: coarseBudget,
      maxObservedBytes: Math.max(0, ...glbFiles.map((file) => file.byteLength)),
      withinBudget,
    },
  };
}

function runPartition(studioBinary, source, output, options, extraArgs) {
  const args = [
    'partition',
    source,
    '--output',
    output,
    '--asset-id',
    'partition-parity',
    '--max-triangles',
    String(options.maxTriangles),
    '--max-bytes',
    String(options.maxBytes),
    '--lod-ratio',
    String(options.lodRatio),
    '--lod-max-triangles',
    String(options.lodMaxTriangles),
    '--lod-min-unit-triangles',
    String(options.lodMinUnitTriangles),
    '--lod-curve-samples',
    String(options.lodCurveSamples),
    '--lod-max-memory-bytes',
    String(options.lodMaxMemoryBytes),
    '--json',
    ...extraArgs,
  ];
  const result = spawnSync(studioBinary, args, {
    cwd: studioRoot,
    encoding: 'utf8',
    timeout: 120_000,
  });
  if (result.error || result.status !== 0) {
    throw new Error(
      `partition failed for ${basename(output)}: `
      + `${result.error?.message ?? result.stderr}`,
    );
  }
  return JSON.parse(result.stdout);
}

function optionsForCase(options, args) {
  const index = args.indexOf('--coarse-chunk-max-bytes');
  return {
    ...options,
    coarseChunkMaxBytes: index >= 0
      ? Number(args[index + 1])
      : options.maxBytes,
  };
}

async function listFiles(root) {
  const output = [];
  async function walk(directory) {
    const entries = await readdir(directory, { withFileTypes: true });
    entries.sort((left, right) => left.name.localeCompare(right.name));
    for (const entry of entries) {
      const absolute = join(directory, entry.name);
      if (entry.isDirectory()) {
        await walk(absolute);
      } else if (entry.isFile()) {
        const bytes = await readFile(absolute);
        output.push({
          path: relative(root, absolute).split('\\').join('/'),
          byteLength: bytes.length,
          digest: sha256(bytes),
          bytes,
        });
      }
    }
  }
  await walk(root);
  return output;
}

function parseGlb(bytes) {
  if (bytes.readUInt32LE(0) !== 0x46546c67) {
    throw new Error('invalid GLB magic');
  }
  let offset = 12;
  let json;
  while (offset < bytes.length) {
    const length = bytes.readUInt32LE(offset);
    const type = bytes.readUInt32LE(offset + 4);
    const data = bytes.subarray(offset + 8, offset + 8 + length);
    if (type === 0x4e4f534a) {
      json = JSON.parse(data.toString('utf8').trim());
    }
    offset += 8 + length;
  }
  if (!json) throw new Error('GLB JSON chunk is missing');
  return json;
}

function summarizeGeometry(files) {
  const accessors = files.flatMap((file) => file.document.accessors ?? []);
  let triangleCount = 0;
  let edgeSegmentCount = 0;
  let edge = null;
  let renderBatchCount = 0;
  let semanticProxyObserved = false;
  const selectionIds = new Set();
  for (const file of files) {
    const json = file.document;
    for (const mesh of json.meshes ?? []) {
      for (const primitive of mesh.primitives ?? []) {
        const mode = primitive.mode ?? 4;
        const accessorIndex = primitive.indices;
        const count = accessorIndex === undefined
          ? 0
          : json.accessors?.[accessorIndex]?.count ?? 0;
        if (mode === 4) {
          triangleCount += Math.floor(count / 3);
        } else if (mode === 1) {
          edgeSegmentCount += Math.floor(count / 2);
          if (!edge) {
            const material = json.materials?.[primitive.material];
            edge = {
              role: primitive.extras?.ice3?.role,
              widthPx: primitive.extras?.ice3?.widthPx,
              color: material?.pbrMetallicRoughness?.baseColorFactor,
              materialId: material?.extras?.ice3?.id,
            };
          }
        }
        if (primitive.attributes?._ICE3_SEMANTIC_INDEX !== undefined) {
          renderBatchCount += 1;
        }
        if (file.path.startsWith('coarse/') && mode === 4) {
          const positionAccessor = primitive.attributes?.POSITION;
          if (json.accessors?.[positionAccessor]?.count === 8
            && Math.floor(count / 3) === 12) {
            semanticProxyObserved = true;
          }
        }
      }
    }
    for (const node of json.nodes ?? []) {
      const selectionId = node.extras?.ice3?.selectionId;
      if (selectionId) selectionIds.add(selectionId);
    }
  }
  return {
    triangleCount,
    accessorCount: accessors.length,
    edge: edgeSegmentCount > 0
      ? { segmentCount: edgeSegmentCount, ...edge }
      : null,
    renderBatchCount,
    semanticProxyObserved,
    selectionIds: [...selectionIds].sort(),
  };
}

function collectUris(root) {
  const output = [];
  function visit(tile) {
    if (tile.content?.uri) output.push(tile.content.uri);
    for (const content of tile.contents ?? []) output.push(content.uri);
    for (const child of tile.children ?? []) visit(child);
  }
  visit(root);
  return output;
}

function collectBounds(root) {
  const output = [];
  function visit(tile) {
    output.push(tile.boundingVolume?.box);
    for (const child of tile.children ?? []) visit(child);
  }
  visit(root);
  return output;
}

function gridObj(objects, size) {
  const lines = ['# ICE3 partition parity fixture'];
  let indexOffset = 1;
  for (let object = 0; object < objects; object += 1) {
    lines.push(`o Unit-${object}`);
    for (let y = 0; y <= size; y += 1) {
      for (let x = 0; x <= size; x += 1) {
        const z = ((x * 3 + y * 5) % 7) * 0.125;
        lines.push(`v ${object * 12 + x} ${y} ${z}`);
      }
    }
    for (let y = 0; y < size; y += 1) {
      for (let x = 0; x < size; x += 1) {
        const a = indexOffset + y * (size + 1) + x;
        const b = a + 1;
        const c = a + size + 1;
        const d = c + 1;
        lines.push(`f ${a} ${b} ${d} ${c}`);
      }
    }
    indexOffset += (size + 1) ** 2;
  }
  return `${lines.join('\n')}\n`;
}

function resolveStudioBinary() {
  if (process.env.ICE3_STUDIO_BIN) return resolve(process.env.ICE3_STUDIO_BIN);
  return resolve(
    studioRoot,
    'target/debug',
    process.platform === 'win32' ? 'studio-cli.exe' : 'studio-cli',
  );
}

function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}
