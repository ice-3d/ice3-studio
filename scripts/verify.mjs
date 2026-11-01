import { spawn } from 'node:child_process';

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
    label: 'Toolchain bundle contract',
    command: 'node',
    args: ['--test', 'scripts/tests/toolchain-bundle.test.mjs'],
  },
  {
    label: 'Toolchain independence',
    command: 'node',
    args: ['scripts/verify-toolchain-independence.mjs'],
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
    label: 'Large reference acceptance contract',
    command: 'node',
    args: ['--test', 'scripts/tests/large-reference-acceptance.test.mjs'],
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
    label: 'WWTP reference example',
    command: 'node',
    args: ['--check', 'scripts/run-wwtp-reference-example.mjs'],
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

function run(command, args) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: process.cwd(),
      env: process.env,
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
  await run(step.command, step.args);
  const seconds = ((performance.now() - started) / 1000).toFixed(2);
  console.log(`PASS ${step.label} (${seconds}s)`);
}

console.log(`\nAll ${steps.length} verification gates passed.`);
