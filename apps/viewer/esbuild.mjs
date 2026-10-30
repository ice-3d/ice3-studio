import { copyFile, mkdir } from 'node:fs/promises';
import { build, context } from 'esbuild';

const watch = process.argv.includes('--watch');
const options = {
  entryPoints: ['src/main.tsx'],
  outdir: 'dist/assets',
  bundle: true,
  splitting: true,
  format: 'esm',
  target: ['chrome120', 'firefox120', 'safari17'],
  sourcemap: true,
  minify: !watch,
  jsx: 'automatic',
  define: {
    __STUDIO_TOKEN__: JSON.stringify(process.env.STUDIO_SESSION_TOKEN ?? ''),
  },
};

await mkdir('dist/assets', { recursive: true });
await copyFile('index.html', 'dist/index.html');
if (watch) {
  const watcher = await context(options);
  await watcher.watch();
  console.log('Workbench watch build active');
} else {
  await build(options);
}
