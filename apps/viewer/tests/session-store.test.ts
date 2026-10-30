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
