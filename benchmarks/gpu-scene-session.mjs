import { spawn } from 'node:child_process';

const child = spawn(
  process.execPath,
  ['benchmarks/large-industrial-scene.mjs'],
  {
    cwd: process.cwd(),
    env: {
      ...process.env,
      STUDIO_GPU_SCENE_SESSIONS: '1',
    },
    stdio: 'inherit',
  },
);

child.once('error', (error) => {
  console.error(error);
  process.exitCode = 1;
});
child.once('exit', (code, signal) => {
  process.exitCode = code ?? (signal ? 1 : 0);
});
