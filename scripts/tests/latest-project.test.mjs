import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

test('latest project selection prefers modification time over revision', async () => {
  const module = await import('../../scripts/run-latest-project.mjs');
  const selected = module.selectLatestProject([
    { projectId: 'older-high-revision', currentRevision: 99, mtimeMs: 1_000 },
    { projectId: 'newest-low-revision', currentRevision: 1, mtimeMs: 3_000 },
    { projectId: 'middle', currentRevision: 8, mtimeMs: 2_000 },
  ]);
  assert.equal(selected.projectId, 'newest-low-revision');
});

test('latest project selection uses revision to break an equal timestamp', async () => {
  const module = await import('../../scripts/run-latest-project.mjs');
  const selected = module.selectLatestProject([
    { projectId: 'same-time-low', currentRevision: 2, mtimeMs: 5_000 },
    { projectId: 'same-time-high', currentRevision: 7, mtimeMs: 5_000 },
  ]);
  assert.equal(selected.projectId, 'same-time-high');
});

test('default development command starts the latest project launcher', async () => {
  const packageJson = JSON.parse(await readFile(
    new URL('../../package.json', import.meta.url),
    'utf8',
  ));
  assert.equal(packageJson.scripts.dev, 'node scripts/run-latest-project.mjs');
});
