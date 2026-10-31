import { createInterface } from 'node:readline';

const mode = process.env.FAKE_SIDECAR_MODE ?? 'normal';
if (mode === 'crash') process.exit(7);

const lines = createInterface({ input: process.stdin });
lines.on('line', (line) => {
  if (mode === 'missing-response') return;
  if (mode === 'invalid-json') {
    process.stdout.write('not-json\n');
    return;
  }
  if (mode === 'oversized') {
    process.stdout.write(`${'x'.repeat(8 * 1024 * 1024 + 1)}\n`);
    return;
  }
  if (mode === 'log-contamination') {
    process.stdout.write('unexpected log line\n');
  }
  const request = JSON.parse(line);
  if (request.method === 'toolchain.capabilities') {
    process.stdout.write(`${JSON.stringify({
      jsonrpc: '2.0',
      id: request.id,
      result: {
        contractVersion: mode === 'incompatible' ? '2.0.0' : '1.0.0',
        packageVersion: '0.2.0',
        operations: [
          'toolchain.capabilities',
          'asset.validate',
          'asset.prepare',
          'asset.preview',
          'asset.partition',
          '$/cancel',
        ],
        inputFormats: ['glb', 'gltf'],
        features: {
          lod: true,
          partition: true,
          tileset: true,
          renderBatching: true,
          cancellation: true,
        },
        limits: {
          maxMessageBytes: 8388608,
          maxConcurrentJobs: 2,
          maxSourceBytes: 2147483648,
        },
      },
    })}\n`);
    return;
  }
  if (request.method === 'asset.preview') {
    process.stdout.write(`${JSON.stringify({
      jsonrpc: '2.0',
      id: request.id,
      result: {
        assetId: request.params.assetId,
        contentHash: `sha256:${'a'.repeat(64)}`,
        profileVersion: 'ice3-prepared-glb-v1',
        manifestPath: `${request.params.outputPath}.manifest.json`,
        primaryPath: request.params.outputPath,
        byteLength: 12,
        diagnostics: [],
        warnings: [],
      },
    })}\n`);
    return;
  }
  process.stdout.write(`${JSON.stringify({
    jsonrpc: '2.0',
    id: request.id,
    error: {
      code: -32601,
      message: 'method not found',
      data: {
        code: 'ICE3_TOOL_SIDECAR_METHOD_NOT_FOUND',
        message: 'method not found',
        retryable: false,
      },
    },
  })}\n`);
});
