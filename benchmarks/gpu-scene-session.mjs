import { spawn } from 'node:child_process';
import { mkdir, writeFile } from 'node:fs/promises';

import { evaluateSceneComparison } from './large-industrial-scene.mjs';

const cpu = await runMode(false);
const gpu = await runMode(true);
const comparison = evaluateSceneComparison(cpu, gpu);
const report = {
  suite: 'gpu-scene-session-comparison',
  cpu,
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
  warmStatsRatio: comparison.warmStatsRatio,
  parameterSpeedup: comparison.parameterSpeedup,
  topologySpeedup: comparison.topologySpeedup,
  failures: comparison.failures,
  passed: comparison.passed,
}, null, 2));
if (!comparison.passed) {
  process.exitCode = 1;
}

function runMode(gpuEnabled) {
  return new Promise((resolve, reject) => {
    const child = spawn(
      process.execPath,
      ['benchmarks/large-industrial-scene.mjs'],
      {
        cwd: process.cwd(),
        env: {
          ...process.env,
          STUDIO_GPU_SCENE_SESSIONS: gpuEnabled ? '1' : '0',
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
        reject(new Error(`benchmark ${gpuEnabled ? 'gpu' : 'cpu'} exited with ${signal ?? code}`));
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
