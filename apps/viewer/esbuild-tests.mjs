import { rm } from 'node:fs/promises';
import { build } from 'esbuild';

await rm('test-dist', { recursive: true, force: true });
await build({
  entryPoints: ['tests/**/*.test.ts'],
  outdir: 'test-dist',
  outbase: 'tests',
  bundle: true,
  packages: 'external',
  platform: 'node',
  format: 'esm',
  target: 'node24',
  sourcemap: true,
});
