import assert from 'node:assert/strict';
import test from 'node:test';
import { SessionStore } from '../src/session/SessionStore';

test('revision events refresh the latest document without changing selection', async () => {
  let revision = 3;
  const store = new SessionStore({
    document: async () => ({
      projectId: 'generic-project',
      revision,
      scene: { nodes: [] },
      components: [],
    }),
    validation: async () => [],
  });
  await store.refresh();
  store.select(['node-000001']);
  revision = 4;
  await store.refresh();

  assert.equal(store.snapshot.revision, 4);
  assert.deepEqual(store.snapshot.selectionIds, ['node-000001']);
});

test('document refresh survives a preview failure', async () => {
  const store = new SessionStore({
    document: async () => ({
      projectId: 'generic-project',
      revision: 4,
      scene: { nodes: [] },
      components: [],
    }),
    validation: async () => [],
    preview: async () => {
      throw new Error('preview failed');
    },
  });

  await store.refresh();

  assert.equal(store.snapshot.revision, 4);
  assert.equal(store.snapshot.connection, 'connected');
  assert.equal(store.snapshot.previewUrl, null);
});

test('loads capabilities without retired driver state', async () => {
  const store = new SessionStore({
    document: async () => ({
      projectId: 'native',
      revision: 0,
      scene: { nodes: [] },
      components: [],
    }),
    validation: async () => [],
    capabilities: async () => ({
      apiVersion: '1.0',
      schemaVersion: 2,
      methods: ['export.runtimePreview'],
      compute: {
        mode: 'cpu',
        selectedBackend: 'cpu',
        adapter: 'reference',
        tier: 'cpu',
        memoryBudgetBytes: 0,
        health: 'ready',
        fallbackReason: null,
        queueDepth: 0,
        maxConcurrentJobs: 1,
        gpuThresholdTriangles: 0,
        lastKernelMs: 0,
        lastUploadBytes: 0,
        lastReadbackBytes: 0,
        lastCacheHits: 0,
      },
    }),
  });

  await store.refresh();

  assert.equal('toolchain' in store.snapshot, false);
});
