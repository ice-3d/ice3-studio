import { rm } from 'node:fs/promises';
import { build } from 'esbuild';

await rm('test-dist', { recursive: true, force: true });
await build({
  entryPoints: ['tests/**/*.test.ts'],
  outdir: 'test-dist',
  outbase: 'tests',
  bundle: false,
  platform: 'node',
  format: 'esm',
  target: 'node24',
  sourcemap: true,
});
