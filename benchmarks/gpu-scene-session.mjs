import { spawn } from 'node:child_process';
import { mkdir, writeFile } from 'node:fs/promises';

import { evaluateSceneComparison } from './large-industrial-scene.mjs';

const fullCpu = await runMode({ gpuEnabled: false, incrementalCache: false });
const incrementalCpu = await runMode({ gpuEnabled: false, incrementalCache: true });
const gpu = await runMode({ gpuEnabled: true, incrementalCache: true });
const comparison = evaluateSceneComparison(fullCpu, incrementalCpu, gpu);
const report = {
  suite: 'gpu-scene-session-comparison',
  fullCpu,
  incrementalCpu,
  gpu,
  comparison,
};

await mkdir('output', { recursive: true });
await writeFile(
  'output/gpu-scene-session-benchmark.json',
  `${JSON.stringify(report, null, 2)}\n`,
);
console.log(JSON.stringify({
  transformSpeedup: comparison.transformSpeedup,
  cpuIncrementalSpeedup: comparison.cpuIncrementalSpeedup,
  gpuOverIncrementalSpeedup: comparison.gpuOverIncrementalSpeedup,
  gpuTotalSpeedup: comparison.gpuTotalSpeedup,
  warmStatsRatio: comparison.warmStatsRatio,
  parameterSpeedup: comparison.parameterSpeedup,
  topologySpeedup: comparison.topologySpeedup,
  visibilitySpeedup: comparison.visibilitySpeedup,
  failures: comparison.failures,
  passed: comparison.passed,
}, null, 2));
if (!comparison.passed) {
  process.exitCode = 1;
}

function runMode({ gpuEnabled, incrementalCache }) {
  return new Promise((resolve, reject) => {
    const child = spawn(
      process.execPath,
      ['benchmarks/large-industrial-scene.mjs'],
      {
        cwd: process.cwd(),
        env: {
          ...process.env,
          STUDIO_GPU_SCENE_SESSIONS: gpuEnabled ? '1' : '0',
          STUDIO_INCREMENTAL_SCENE_CACHE: incrementalCache ? '1' : '0',
        },
        stdio: ['ignore', 'pipe', 'inherit'],
      },
    );
    let stdout = '';
    child.stdout.setEncoding('utf8');
    child.stdout.on('data', (chunk) => {
      stdout += chunk;
    });
    child.once('error', reject);
    child.once('exit', (code, signal) => {
      if (code !== 0) {
        reject(new Error(
          `benchmark ${gpuEnabled ? 'gpu' : incrementalCache ? 'cpu-incremental' : 'cpu-full'} exited with ${signal ?? code}`,
        ));
        return;
      }
      const marker = 'BENCHMARK_JSON:';
      const start = stdout.lastIndexOf(marker);
      if (start < 0) {
        reject(new Error('benchmark did not emit machine-readable evidence'));
        return;
      }
      const line = stdout.slice(start + marker.length).split(/\r?\n/, 1)[0];
      resolve(JSON.parse(line));
    });
  });
}
