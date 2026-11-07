import assert from 'node:assert/strict';
import test from 'node:test';
import { ChangeSetStore } from '../src/changeset/ChangeSetStore';

const diff = {
  addedNodeIds: [], updatedNodeIds: [], removedNodeIds: [],
  addedMaterialIds: [], updatedMaterialIds: [], removedMaterialIds: [],
  addedRelationIds: [], updatedRelationIds: [], removedRelationIds: [],
  addedConstraintIds: [], updatedConstraintIds: [], removedConstraintIds: [],
};

test('blocks approval when evaluation changes the digest', async () => {
  const client = {
    createChangeSet: async () => ({ id: 'changeset-0001', version: 1 }),
    addChangeSetOperations: async () => ({ version: 2 }),
    removeChangeSetOperations: async () => ({ version: 2 }),
    evaluateChangeSet: async () => ({
      changeSetId: 'changeset-0001',
      version: 2,
      baseRevision: 0,
      status: 'ready',
      candidateDigest: 'sha256:new',
      diff,
      diagnostics: [],
    }),
    commitChangeSet: async () => ({ revision: 1 }),
  };
  const store = new ChangeSetStore(client);
  store.acceptEvaluation({
    changeSetId: 'changeset-0001',
    version: 2,
    baseRevision: 0,
    status: 'ready',
    candidateDigest: 'sha256:old',
    diff,
    diagnostics: [],
  });
  store.markReviewed('sha256:old');

  await store.evaluate();

  assert.equal(store.snapshot.canApprove, false);
  assert.equal(store.snapshot.status, 'ready');
});

test('loads the impact estimate after evaluating a proposal', async () => {
  const client = {
    createChangeSet: async () => ({ id: 'changeset-0002', version: 1 }),
    addChangeSetOperations: async () => ({ version: 2 }),
    removeChangeSetOperations: async () => ({ version: 2 }),
    evaluateChangeSet: async () => ({
      changeSetId: 'changeset-0002',
      version: 2,
      baseRevision: 0,
      status: 'ready',
      candidateDigest: 'sha256:candidate',
      diff,
      diagnostics: [],
    }),
    commitChangeSet: async () => ({ revision: 1 }),
    changeSetImpact: async (changeSetId: string, version: number) => ({
      changeSetId,
      version,
      baseRevision: 0,
      status: 'ready',
      candidateDigest: 'sha256:candidate',
      diagnosticCount: 0,
      impact: {
        scope: 'local',
        fullSceneRebuildRequired: false,
        sourceNodes: { added: 1, updated: 0, removed: 0 },
        derivedNodes: { added: 0, updated: 0, removed: 0 },
        relations: { added: 0, updated: 0, removed: 0 },
        components: { added: 0, updated: 0, removed: 0 },
        materials: { added: 0, updated: 0, removed: 0 },
      },
    }),
  };
  const store = new ChangeSetStore(client);
  store.acceptEvaluation({
    changeSetId: 'changeset-0002',
    version: 1,
    baseRevision: 0,
    status: 'ready',
    candidateDigest: 'sha256:candidate',
    diff,
    diagnostics: [],
  });

  await store.evaluate();

  assert.equal(store.snapshot.impact?.impact.scope, 'local');
  assert.equal(store.snapshot.impact?.impact.sourceNodes.added, 1);
});

test('keeps reviewing when the impact estimate fails', async () => {
  const client = {
    createChangeSet: async () => ({ id: 'changeset-0003', version: 1 }),
    addChangeSetOperations: async () => ({ version: 2 }),
    removeChangeSetOperations: async () => ({ version: 2 }),
    evaluateChangeSet: async () => ({
      changeSetId: 'changeset-0003',
      version: 2,
      baseRevision: 0,
      status: 'ready',
      candidateDigest: 'sha256:candidate',
      diff,
      diagnostics: [],
    }),
    commitChangeSet: async () => ({ revision: 1 }),
    changeSetImpact: async () => {
      throw new Error('impact unavailable');
    },
  };
  const store = new ChangeSetStore(client);
  store.acceptEvaluation({
    changeSetId: 'changeset-0003',
    version: 1,
    baseRevision: 0,
    status: 'ready',
    candidateDigest: 'sha256:candidate',
    diff,
    diagnostics: [],
  });

  await store.evaluate();

  assert.equal(store.snapshot.impact, null);
  assert.equal(store.snapshot.status, 'ready');
});
