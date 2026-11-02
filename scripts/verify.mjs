import { spawn } from 'node:child_process';
import { resolve } from 'node:path';

const studioCli = resolve(
  'target/debug',
  process.platform === 'win32' ? 'studio-cli.exe' : 'studio-cli',
);

const steps = [
  {
    label: 'Rust format',
    command: 'cargo',
    args: ['fmt', '--check'],
  },
  {
    label: 'Rust lint',
    command: 'cargo',
    args: ['clippy', '--workspace', '--all-targets', '--', '-D', 'warnings'],
  },
  {
    label: 'Rust tests',
    command: 'cargo',
    args: ['test', '--workspace'],
  },
  {
    label: 'JT source import contract',
    command: 'cargo',
    args: [
      'test',
      '-p',
      'studio-import',
      '--test',
      'jt_registry',
      '--test',
      'jt_converter',
      '--test',
      'jt_import',
    ],
  },
  {
    label: 'JT CLI isolation contract',
    command: 'cargo',
    args: [
      'test',
      '-p',
      'studio-cli',
      '--test',
      'jt_import',
      '--test',
      'jt_failures',
    ],
  },
  {
    label: 'JT native parity',
    command: 'node',
    args: ['scripts/compare-jt-imports.mjs'],
  },
  {
    label: 'JT parity report contract',
    command: 'node',
    args: ['--test', 'scripts/tests/jt-import-contract.test.mjs'],
  },
  {
    label: 'Studio CLI build',
    command: 'cargo',
    args: ['build', '-p', 'studio-cli'],
  },
  {
    label: 'Prepare baseline contract',
    command: 'node',
    args: ['--test', 'scripts/tests/prepare-baseline.test.mjs'],
  },
  {
    label: 'Prepare native parity',
    command: 'node',
    args: ['scripts/compare-prepare-imports.mjs'],
    env: { ICE3_STUDIO_BIN: studioCli },
  },
  {
    label: 'Studio standalone',
    command: 'node',
    args: ['scripts/verify-studio-standalone.mjs'],
    env: { STUDIO_CLI_BIN: studioCli },
  },
  {
    label: 'Generated schemas',
    command: 'cargo',
    args: [
      'run',
      '-q',
      '-p',
      'studio-cli',
      '--',
      'schema',
      '--output',
      'schemas',
      '--check',
    ],
  },
  {
    label: 'AI fixture API flow',
    command: 'node',
    args: ['scripts/run-ai-fixture.mjs', '--fixture', 'fixtures/wastewater-mini'],
  },
  {
    label: 'Wastewater plant asset contract',
    command: 'node',
    args: ['--test', 'scripts/tests/wastewater-plant-model.test.mjs'],
  },
  {
    label: 'Large industrial fixture contract',
    command: 'node',
    args: [
      '--test',
      'scripts/tests/large-industrial-detail.test.mjs',
      'scripts/tests/large-industrial-fixture.test.mjs',
      'scripts/tests/reference-layout.test.mjs',
    ],
  },
  {
    label: 'Large industrial fixture',
    command: 'node',
    args: ['scripts/run-large-industrial-fixture.mjs'],
  },
  {
    label: 'Native GPU benchmark script',
    command: 'node',
    args: ['--check', 'benchmarks/native-gpu.mjs'],
  },
  {
    label: 'GPU scene benchmark script',
    command: 'node',
    args: ['--check', 'benchmarks/gpu-scene-session.mjs'],
  },
  {
    label: 'Latest project launcher contract',
    command: 'node',
    args: ['--test', 'scripts/tests/latest-project.test.mjs'],
  },
  {
    label: 'Viewer unit tests',
    command: 'npm',
    args: ['--prefix', 'apps/viewer', 'test'],
  },
  {
    label: 'Viewer typecheck',
    command: 'npm',
    args: ['--prefix', 'apps/viewer', 'run', 'typecheck'],
  },
  {
    label: 'Viewer production build',
    command: 'npm',
    args: ['--prefix', 'apps/viewer', 'run', 'build'],
  },
  {
    label: 'Viewer end-to-end tests',
    command: 'npm',
    args: ['--prefix', 'apps/viewer', 'run', 'test:e2e'],
  },
  {
    label: 'Viewer dependency audit',
    command: 'npm',
    args: ['--prefix', 'apps/viewer', 'audit', '--audit-level', 'high'],
  },
];

function run(command, args, extraEnv = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: process.cwd(),
      env: { ...process.env, ...extraEnv },
      stdio: 'inherit',
    });
    child.once('error', reject);
    child.once('exit', (code, signal) => {
      if (code === 0) {
        resolve();
        return;
      }
      reject(new Error(`${command} exited with ${signal ?? code}`));
    });
  });
}

for (const [index, step] of steps.entries()) {
  const started = performance.now();
  console.log(`\n[${index + 1}/${steps.length}] ${step.label}`);
  await run(step.command, step.args, step.env);
  const seconds = ((performance.now() - started) / 1000).toFixed(2);
  console.log(`PASS ${step.label} (${seconds}s)`);
}

console.log(`\nAll ${steps.length} verification gates passed.`);
