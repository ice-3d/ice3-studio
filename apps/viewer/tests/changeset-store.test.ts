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
