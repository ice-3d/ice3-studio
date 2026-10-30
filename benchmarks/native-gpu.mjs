import { spawn } from 'node:child_process';
import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

const marker = 'BENCHMARK_JSON:';
const result = await run('cargo', [
  'test',
  '-p',
  'studio-gpu',
  '--release',
  '--test',
  'native_gpu_benchmark',
  '--',
  '--ignored',
  '--nocapture',
]);
const markerIndex = result.stdout.lastIndexOf(marker);
if (markerIndex < 0) {
  process.stderr.write(result.stderr);
  throw new Error('native GPU benchmark did not emit a JSON report');
}
const reportLine = result.stdout.slice(markerIndex + marker.length).split(/\r?\n/, 1)[0];
const report = JSON.parse(reportLine);
const outputDirectory = path.resolve('output');
await mkdir(outputDirectory, { recursive: true });
const outputPath = path.join(outputDirectory, 'native-gpu-benchmark.json');
await writeFile(outputPath, `${JSON.stringify(report, null, 2)}\n`, 'utf8');

if (report.skipped) {
  console.log(`SKIP native GPU benchmark: ${report.reason}`);
  process.exit(0);
}
for (const workload of report.workloads) {
  console.log(
    `${workload.triangles.toLocaleString()} tris: `
      + `CPU ${workload.cpu.p50Ms.toFixed(2)} ms, `
      + `GPU ${workload.gpu.p50Ms.toFixed(2)} ms, `
      + `gain ${(workload.p50Gain * 100).toFixed(1)}%, `
      + `parity ${workload.parity}`,
  );
}
console.log(`Evidence: ${outputPath}`);
if (result.code !== 0 || !report.ok) {
  process.stderr.write(result.stderr);
  process.exit(result.code || 1);
}

function run(command, args) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: process.cwd(),
      env: process.env,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    let stdout = '';
    let stderr = '';
    child.stdout.setEncoding('utf8');
    child.stderr.setEncoding('utf8');
    child.stdout.on('data', (chunk) => {
      stdout += chunk;
    });
    child.stderr.on('data', (chunk) => {
      stderr += chunk;
    });
    child.once('error', reject);
    child.once('exit', (code) => resolve({ code, stdout, stderr }));
  });
}
