import assert from 'node:assert/strict';
import test from 'node:test';
import type { ChangeSetRecord } from '../src/api/StudioClient';
import { ChangeSetStore } from '../src/changeset/ChangeSetStore';
import type { ChangeSetClient, ChangeSetEvaluation } from '../src/changeset/types';

const diff = {
  addedNodeIds: [], updatedNodeIds: [], removedNodeIds: [],
  addedMaterialIds: [], updatedMaterialIds: [], removedMaterialIds: [],
  addedRelationIds: [], updatedRelationIds: [], removedRelationIds: [],
  addedConstraintIds: [], updatedConstraintIds: [], removedConstraintIds: [],
};

const changeSet: ChangeSetRecord = {
  schemaVersion: 1,
  id: 'changeset-0004',
  projectId: 'generic-project',
  baseRevision: 2,
  version: 2,
  status: 'ready',
  title: 'Workbench form edits',
  entries: [{
    id: 'entry-000001',
    source: { kind: 'human', label: 'Inspector' },
    operation: {
      type: 'node.update',
      nodeId: 'node-000010',
      name: 'Clarifier',
    },
    createdAt: 1,
  }],
  candidateDigest: 'sha256:candidate',
  diagnostics: [],
  committedRevision: null,
  commitResult: null,
  commitIdempotencyKey: null,
  createdAt: 1,
  updatedAt: 1,
};

test('blocks approval when evaluation changes the digest', async () => {
  const client: ChangeSetClient = {
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
  const client: ChangeSetClient = {
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

test('removes an entry and evaluates the new ChangeSet version', async () => {
  const calls: string[] = [];
  const client: ChangeSetClient = {
    createChangeSet: async () => ({ id: 'changeset-0004', version: 1 }),
    addChangeSetOperations: async () => ({ version: 2 }),
    removeChangeSetOperations: async (
      _changeSetId: string,
      expectedVersion: number,
      entryIds: string[],
    ) => {
      calls.push(`remove:${expectedVersion}:${entryIds.join(',')}`);
      return { version: 3 };
    },
    evaluateChangeSet: async (_changeSetId: string, expectedVersion: number) => {
      calls.push(`evaluate:${expectedVersion}`);
      return {
        changeSetId: 'changeset-0004',
        version: expectedVersion,
        baseRevision: 2,
        status: 'blocked',
        candidateDigest: null,
        diff,
        diagnostics: [{
          code: 'ICE3_STUDIO_OPERATION_INVALID',
          operationIndex: null,
          objectId: null,
          pointer: '/entries',
          message: 'ChangeSet has no operations',
        }],
      };
    },
    changeSet: async () => ({
      ...changeSet,
      version: 3,
      status: 'blocked',
      entries: [],
      candidateDigest: null,
      diagnostics: [{
        code: 'ICE3_STUDIO_OPERATION_INVALID',
        operationIndex: null,
        objectId: null,
        pointer: '/entries',
        message: 'ChangeSet has no operations',
      }],
    }),
    commitChangeSet: async () => ({ revision: 3 }),
  };
  const store = new ChangeSetStore(client);
  store.acceptEvaluation({
    changeSetId: 'changeset-0004',
    version: 2,
    baseRevision: 2,
    status: 'ready',
    candidateDigest: 'sha256:candidate',
    diff,
    diagnostics: [],
  });
  store.updateRecord(changeSet);

  await store.removeEntry('entry-000001');

  assert.deepEqual(calls, ['remove:2:entry-000001', 'evaluate:3']);
  assert.equal(store.snapshot.record?.entries.length, 0);
  assert.equal(store.snapshot.status, 'blocked');
  assert.equal(store.snapshot.candidateUrl, null);
});

test('clears a previous candidate preview when the next evaluation is blocked', async () => {
  const client: ChangeSetClient = {
    createChangeSet: async () => ({ id: 'changeset-0005', version: 1 }),
    addChangeSetOperations: async () => ({ version: 2 }),
    removeChangeSetOperations: async () => ({ version: 3 }),
    evaluateChangeSet: async () => ({
      changeSetId: 'changeset-0005',
      version: 3,
      baseRevision: 2,
      status: 'blocked',
      candidateDigest: null,
      diff,
      diagnostics: [{
        code: 'ICE3_STUDIO_OPERATION_INVALID',
        operationIndex: 0,
        objectId: 'node-000010',
        pointer: '/nodes/node-000010/name',
        message: 'invalid name',
      }],
    }),
    changeSet: async () => ({
      ...changeSet,
      id: 'changeset-0005',
      version: 3,
      status: 'blocked',
      candidateDigest: null,
    }),
    commitChangeSet: async () => ({ revision: 3 }),
  };
  const store = new ChangeSetStore(client);
  store.acceptEvaluation({
    changeSetId: 'changeset-0005',
    version: 2,
    baseRevision: 2,
    status: 'ready',
    candidateDigest: 'sha256:old',
    diff,
    diagnostics: [],
  });
  store.updateRecord({ ...changeSet, id: 'changeset-0005', version: 2 });
  store.updateCandidatePreview('blob:old-candidate');

  await store.evaluate();

  assert.equal(store.snapshot.candidateUrl, null);
  assert.equal(store.snapshot.status, 'blocked');
});

test('withdraws the previous candidate preview while reevaluation is pending', async () => {
  let finishEvaluation!: (evaluation: ChangeSetEvaluation) => void;
  const pendingEvaluation = new Promise<ChangeSetEvaluation>((resolve) => {
    finishEvaluation = resolve;
  });
  const client: ChangeSetClient = {
    createChangeSet: async () => ({ id: 'changeset-0006', version: 1 }),
    addChangeSetOperations: async () => ({ version: 2 }),
    removeChangeSetOperations: async () => ({ version: 3 }),
    evaluateChangeSet: async () => pendingEvaluation,
    commitChangeSet: async () => ({ revision: 3 }),
  };
  const store = new ChangeSetStore(client);
  store.acceptEvaluation({
    changeSetId: 'changeset-0006',
    version: 2,
    baseRevision: 2,
    status: 'ready',
    candidateDigest: 'sha256:old',
    diff,
    diagnostics: [],
  });
  store.updateRecord({ ...changeSet, id: 'changeset-0006', version: 2 });
  store.updateCandidatePreview('blob:old-candidate');

  const pending = store.evaluate();

  assert.equal(store.snapshot.candidateUrl, null);
  finishEvaluation({
    changeSetId: 'changeset-0006',
    version: 2,
    baseRevision: 2,
    status: 'ready',
    candidateDigest: 'sha256:new',
    diff,
    diagnostics: [],
  });
  await pending;
});

test('closes the active candidate after a successful commit', async () => {
  const client: ChangeSetClient = {
    createChangeSet: async () => ({ id: 'changeset-0007', version: 1 }),
    addChangeSetOperations: async () => ({ version: 2 }),
    removeChangeSetOperations: async () => ({ version: 3 }),
    evaluateChangeSet: async () => ({
      changeSetId: 'changeset-0007',
      version: 2,
      baseRevision: 2,
      status: 'ready',
      candidateDigest: 'sha256:candidate',
      diff,
      diagnostics: [],
    }),
    changeSet: async () => ({
      ...changeSet,
      id: 'changeset-0007',
      status: 'committed',
      committedRevision: 3,
    }),
    commitChangeSet: async () => ({ revision: 3 }),
  };
  const store = new ChangeSetStore(client);
  store.acceptEvaluation({
    changeSetId: 'changeset-0007',
    version: 2,
    baseRevision: 2,
    status: 'ready',
    candidateDigest: 'sha256:candidate',
    diff,
    diagnostics: [],
  });
  store.updateRecord({ ...changeSet, id: 'changeset-0007', version: 2 });
  store.updateCandidatePreview('blob:candidate');
  store.markReviewed('sha256:candidate');

  await store.commit();

  assert.equal(store.snapshot.status, 'committed');
  assert.equal(store.snapshot.active, null);
  assert.equal(store.snapshot.candidateUrl, null);
  assert.equal(store.snapshot.canApprove, false);
});

test('discards the active candidate and clears its preview', async () => {
  let discardedId: string | null = null;
  const client: ChangeSetClient = {
    createChangeSet: async () => ({ id: 'changeset-0008', version: 1 }),
    addChangeSetOperations: async () => ({ version: 2 }),
    removeChangeSetOperations: async () => ({ version: 3 }),
    evaluateChangeSet: async () => ({
      changeSetId: 'changeset-0008',
      version: 2,
      baseRevision: 2,
      status: 'ready',
      candidateDigest: 'sha256:candidate',
      diff,
      diagnostics: [],
    }),
    commitChangeSet: async () => ({ revision: 3 }),
    discardChangeSet: async (changeSetId: string) => {
      discardedId = changeSetId;
      return { ...changeSet, id: changeSetId, status: 'discarded' };
    },
  };
  const store = new ChangeSetStore(client);
  store.acceptEvaluation({
    changeSetId: 'changeset-0008',
    version: 2,
    baseRevision: 2,
    status: 'ready',
    candidateDigest: 'sha256:candidate',
    diff,
    diagnostics: [],
  });
  store.updateCandidatePreview('blob:candidate');

  await store.discard();

  assert.equal(discardedId, 'changeset-0008');
  assert.equal(store.snapshot.active, null);
  assert.equal(store.snapshot.candidateUrl, null);
  assert.equal(store.snapshot.status, 'discarded');
});
