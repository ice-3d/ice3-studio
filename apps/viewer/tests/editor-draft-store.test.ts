import assert from 'node:assert/strict';
import test from 'node:test';
import type { StudioNode } from '../src/api/StudioClient';
import { EditorDraftStore } from '../src/editor/EditorDraftStore';
import { createEditorValues } from '../src/editor/types';

const node: StudioNode = {
  id: 'node-000001',
  name: 'Tank',
  parentId: null,
  visible: true,
  metadata: {},
  transform: {
    translation: { x: 0, y: 0, z: 0 },
    rotation: [0, 0, 0, 1],
    scale: { x: 1, y: 1, z: 1 },
  },
  content: { type: 'primitive', kind: 'box', width: 1, height: 1, depth: 1 },
};

test('commits clean, undo and redo values', () => {
  const store = new EditorDraftStore();
  const initial = store.ensure(node, 1);
  store.commit(node.id, { ...initial, name: 'Tank A' });
  assert.equal(store.snapshot.drafts[node.id]?.values.name, 'Tank A');
  assert.equal(store.snapshot.drafts[node.id]?.status, 'dirty');
  store.undo(node.id);
  assert.equal(store.snapshot.drafts[node.id]?.values.name, 'Tank');
  store.redo(node.id);
  assert.equal(store.snapshot.drafts[node.id]?.values.name, 'Tank A');
});

test('records one history entry for a continuous preview gesture', () => {
  const store = new EditorDraftStore();
  const initial = store.ensure(node, 1);
  store.beginPreview(node.id);
  store.updatePreview(node.id, {
    ...initial,
    transform: { ...initial.transform, translation: { x: 1, y: 0, z: 0 } },
  });
  store.updatePreview(node.id, {
    ...initial,
    transform: { ...initial.transform, translation: { x: 2, y: 0, z: 0 } },
  });
  store.finishPreview(node.id);
  store.undo(node.id);
  assert.equal(
    store.snapshot.drafts[node.id]?.values.transform.translation.x,
    0,
  );
});

test('cancel preview restores the gesture start without history', () => {
  const store = new EditorDraftStore();
  const initial = store.ensure(node, 1);
  store.beginPreview(node.id);
  store.updatePreview(node.id, {
    ...initial,
    transform: { ...initial.transform, translation: { x: 9, y: 0, z: 0 } },
  });
  store.cancelPreview(node.id);
  assert.equal(store.snapshot.drafts[node.id]?.values.transform.translation.x, 0);
  assert.equal(store.snapshot.drafts[node.id]?.history.length, 0);
});

test('marks revision mismatch stale and retains staged values until clear', () => {
  const store = new EditorDraftStore();
  store.ensure(node, 1);
  store.markStaged(node.id);
  assert.equal(store.snapshot.drafts[node.id]?.status, 'staged');
  store.markStale(node.id, 'Project advanced');
  assert.equal(store.snapshot.drafts[node.id]?.status, 'stale');
  store.clear(node.id);
  assert.equal(store.snapshot.drafts[node.id], undefined);
});

test('marks Stage failure without discarding values', () => {
  const store = new EditorDraftStore();
  const initial = store.ensure(node, 1);
  const changed = { ...initial, name: 'Tank B' };
  store.commit(node.id, changed);
  store.markStageFailed(node.id, 'Server rejected candidate');
  const draft = store.snapshot.drafts[node.id]!;
  assert.equal(draft.status, 'failed');
  assert.equal(draft.values.name, 'Tank B');
  assert.equal(draft.error, 'Server rejected candidate');
});

test('restores staged drafts to dirty and reports unsaved work', () => {
  const store = new EditorDraftStore();
  store.ensure(node, 1);
  store.markStaged(node.id);
  assert.equal(store.hasDirtyDrafts(), true);
  store.restoreStagedAsDirty();
  assert.equal(store.snapshot.drafts[node.id]?.status, 'dirty');
});

test('keeps the staged transform as the candidate preview baseline', () => {
  const store = new EditorDraftStore();
  const initial = store.ensure(node, 1);
  const staged = {
    ...initial,
    transform: {
      ...initial.transform,
      translation: { x: 2, y: 0, z: 0 },
    },
  };
  store.commit(node.id, staged);
  store.markStaged(node.id);
  const edited = {
    ...staged,
    transform: {
      ...staged.transform,
      translation: { x: 4, y: 0, z: 0 },
    },
  };
  store.commit(node.id, edited);
  assert.deepEqual(
    store.snapshot.drafts[node.id]?.stagedValues?.transform,
    staged.transform,
  );
  store.restoreStagedAsDirty();
  assert.equal(store.snapshot.drafts[node.id]?.stagedValues, null);
});

test('uses createEditorValues as the baseline helper', () => {
  assert.deepEqual(createEditorValues(node).transform, node.transform);
});

test('store snapshots are replaced so React consumers rerender', () => {
  const store = new EditorDraftStore();
  const before = store.snapshot;
  store.ensure(node, 1);
  assert.notEqual(store.snapshot, before);
});
