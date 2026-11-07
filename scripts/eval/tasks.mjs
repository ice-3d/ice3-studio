// Agent evaluation tasks.
//
// Each task drives the public JSON-RPC surface and asserts on the result. The
// reference client that runs them is the standard answer, so a green run proves
// the task is solvable and the grader is wired correctly — not that a language
// model can do it. Swapping in a model means implementing the same client
// surface; the graders stay untouched.

const IDENTITY = {
  translation: { x: 0, y: 0, z: 0 },
  rotation: [0, 0, 0, 1],
  scale: { x: 1, y: 1, z: 1 },
};

function boxNode(nodeId, name, size = 2) {
  return {
    type: 'node.create',
    nodeId,
    parentId: null,
    name,
    transform: IDENTITY,
    content: { type: 'primitive', kind: 'box', width: size, height: size, depth: size },
    materialId: null,
    metadata: {},
  };
}

async function expectFailure(action) {
  try {
    await action();
    return { failed: false };
  } catch (error) {
    return { failed: true, code: error.code, message: error.message };
  }
}

async function createChangeSet(client, idempotencyKey, title = 'Eval proposal') {
  return client.call('changeset.create', {
    baseRevision: (await client.call('document.get', {})).revision,
    title,
    idempotencyKey,
  });
}

export const tasks = [
  {
    id: 'context-summary',
    goal: 'Read the scene summary and confirm project, units and a committed revision.',
    async run({ client }) {
      return client.call('scene.context', {});
    },
    check(result, { assert }) {
      assert(typeof result.projectId === 'string' && result.projectId.length > 0, 'projectId');
      assert(result.revision > 0, `revision should be committed, got ${result.revision}`);
      assert(result.units === 'meters', 'units');
      assert(result.coordinateSystem.upAxis === 'Y', 'up axis');
      assert(result.counts.sourceNodes === 11, `expected 11 source nodes, got ${result.counts.sourceNodes}`);
    },
  },
  {
    id: 'search-by-name',
    goal: 'Find the aeration tanks by name.',
    async run({ client }) {
      return client.call('scene.search', { query: 'Tank' });
    },
    check(result, { assert }) {
      assert(result.items.length === 2, `expected 2 tanks, got ${result.items.length}`);
      assert(result.items.every((item) => item.name.includes('Tank')), 'every hit names a tank');
      assert(result.nextCursor === null, 'a full page should not page further');
    },
  },
  {
    id: 'search-pagination',
    goal: 'Page through the whole scene without duplicate or lost entries.',
    async run({ client }) {
      const first = await client.call('scene.search', { limit: 4 });
      const second = await client.call('scene.search', { limit: 4, cursor: first.nextCursor });
      const third = await client.call('scene.search', { limit: 4, cursor: second.nextCursor });
      return { pages: [first, second, third] };
    },
    check(result, { assert }) {
      const ids = result.pages.flatMap((page) => page.items.map((item) => item.id));
      assert(ids.length === 11, `expected 11 rows across pages, got ${ids.length}`);
      assert(new Set(ids).size === ids.length, 'pages must not repeat a node');
      assert(result.pages[2].nextCursor === null, 'the last page must end the cursor');
    },
  },
  {
    id: 'search-projection',
    goal: 'Project only the requested fields.',
    async run({ client }) {
      return client.call('scene.search', { limit: 3, fields: ['id', 'scope', 'kind', 'name'] });
    },
    check(result, { assert }) {
      assert(result.items.length === 3, 'three rows');
      for (const item of result.items) {
        const keys = Object.keys(item).sort();
        assert(
          keys.join(',') === 'id,kind,name,scope',
          `projection leaked extra fields: ${keys.join(',')}`,
        );
      }
    },
  },
  {
    id: 'search-kind-filter',
    goal: 'List only the swept pipe routes.',
    async run({ client }) {
      return client.call('scene.search', { contentKind: 'sweep' });
    },
    check(result, { assert }) {
      assert(result.items.length === 4, `expected 4 sweeps, got ${result.items.length}`);
      assert(result.items.every((item) => item.kind === 'sweep'), 'only sweeps');
    },
  },
  {
    id: 'measure-distance',
    goal: 'Measure the distance between the ground and a tank.',
    async run({ client }) {
      return client.call('scene.measure', {
        from: { nodeId: 'node-000001' },
        to: { nodeId: 'node-000002' },
      });
    },
    check(result, { assert }) {
      assert(result.distance > 0, `distance should be positive, got ${result.distance}`);
      assert(result.from.length === 3 && result.to.length === 3, 'resolved endpoints');
      assert(result.clearance <= result.distance, 'clearance cannot exceed centre distance');
    },
  },
  {
    id: 'bom-grouping',
    goal: 'Group the evaluated scene into a bill of materials.',
    async run({ client }) {
      return client.call('scene.bom', { groupBy: 'material' });
    },
    check(result, { assert }) {
      assert(result.rows.length > 0, 'the scene has materials');
      assert(result.groupBy === 'material', `groupBy echoed back, got ${result.groupBy}`);
      assert(result.total >= result.rows.length, 'total counts every group');
    },
  },
  {
    id: 'clash-audit',
    goal: 'Run a clearance audit and get a structured answer.',
    async run({ client }) {
      return client.call('scene.clashes', { clearance: 0 });
    },
    check(result, { assert }) {
      assert(Array.isArray(result.pairs), 'pairs is an array');
      assert(result.clearance === 0, 'clearance echoed back');
      assert(typeof result.truncated === 'boolean', 'truncated flag');
    },
  },
  {
    id: 'evaluation-query',
    goal: 'Read the evaluated summary of one node.',
    async run({ client }) {
      return client.call('evaluation.query', {
        nodeIds: ['node-000002'],
        include: ['summary'],
      });
    },
    check(result, { assert }) {
      assert(result.items.length === 1, 'one item per requested node');
      assert(result.items[0].nodeId === 'node-000002', 'requested node echoed back');
      assert(result.items[0].indexCount > 0, 'a box contributes triangle indices');
      assert(typeof result.items[0].geometryId === 'string', 'geometry id is content addressed');
    },
  },
  {
    id: 'changeset-create',
    goal: 'Open a proposal on the committed revision.',
    async run({ client }) {
      return createChangeSet(client, 'eval-create');
    },
    check(result, { assert }) {
      assert(/^changeset-[0-9a-f]{16}$/.test(result.id), `unexpected id ${result.id}`);
      assert(result.version === 0, `new proposal starts at version 0, got ${result.version}`);
      assert(result.status === 'draft', 'new proposal is a draft');
      assert(result.entries.length === 0, 'no entries yet');
    },
  },
  {
    id: 'changeset-add-node',
    goal: 'Append one node to the proposal.',
    async run({ client }) {
      const changeSet = await createChangeSet(client, 'eval-add-create');
      return client.call('changeset.addOperations', {
        changeSetId: changeSet.id,
        expectedVersion: changeSet.version,
        idempotencyKey: 'eval-add',
        entries: [{
          source: { kind: 'agent', label: 'eval' },
          operation: boxNode('node-900001', 'Eval Tank'),
        }],
      });
    },
    check(result, { assert }) {
      assert(result.version === 1, `version should advance to 1, got ${result.version}`);
      assert(result.entries.length === 1, 'one entry');
      assert(result.entries[0].id === 'entry-000001', `entry id ${result.entries[0].id}`);
    },
  },
  {
    id: 'changeset-evaluate',
    goal: 'Evaluate the candidate and obtain a digest for review.',
    async run({ client }) {
      const changeSet = await createChangeSet(client, 'eval-eval-create');
      const added = await client.call('changeset.addOperations', {
        changeSetId: changeSet.id,
        expectedVersion: changeSet.version,
        idempotencyKey: 'eval-eval-add',
        entries: [{
          source: { kind: 'agent', label: 'eval' },
          operation: boxNode('node-900002', 'Evaluate Me'),
        }],
      });
      const evaluation = await client.call('changeset.evaluate', {
        changeSetId: changeSet.id,
        expectedVersion: added.version,
        includeCandidate: true,
      });
      return evaluation;
    },
    check(result, { assert }) {
      assert(result.status === 'ready', `expected ready, got ${result.status}`);
      assert(/^[0-9a-f]{64}$/.test(result.candidateDigest), 'digest is a sha256 hex string');
      assert(result.diagnostics.length === 0, 'a valid box produces no diagnostics');
    },
  },
  {
    id: 'changeset-impact',
    goal: 'Predict the cost of the candidate before asking a human to review it.',
    async run({ client }) {
      const changeSet = await createChangeSet(client, 'eval-impact-create');
      const added = await client.call('changeset.addOperations', {
        changeSetId: changeSet.id,
        expectedVersion: changeSet.version,
        idempotencyKey: 'eval-impact-add',
        entries: [{
          source: { kind: 'agent', label: 'eval' },
          operation: boxNode('node-900003', 'Impact Box'),
        }],
      });
      await client.call('changeset.evaluate', {
        changeSetId: changeSet.id,
        expectedVersion: added.version,
        includeCandidate: true,
      });
      return client.call('changeset.impact', {
        changeSetId: changeSet.id,
        expectedVersion: added.version,
      });
    },
    check(result, { assert }) {
      assert(result.status === 'ready', `expected ready, got ${result.status}`);
      assert(typeof result.impact === 'object' && result.impact !== null, 'impact payload');
      assert(typeof result.impact.scope === 'string', 'impact scope');
      assert(result.impact.sourceNodes.added >= 1, 'the new node should be counted as added');
    },
  },
  {
    id: 'changeset-remove',
    goal: 'Remove an entry and keep the proposal consistent.',
    async run({ client }) {
      const changeSet = await createChangeSet(client, 'eval-remove-create');
      const added = await client.call('changeset.addOperations', {
        changeSetId: changeSet.id,
        expectedVersion: changeSet.version,
        idempotencyKey: 'eval-remove-add',
        entries: [{
          source: { kind: 'agent', label: 'eval' },
          operation: boxNode('node-900004', 'Remove Me'),
        }],
      });
      return client.call('changeset.removeOperations', {
        changeSetId: changeSet.id,
        expectedVersion: added.version,
        entryIds: [added.entries[0].id],
        idempotencyKey: 'eval-remove',
      });
    },
    check(result, { assert }) {
      assert(result.entries.length === 0, 'the proposal is empty again');
      assert(result.version === 2, `version advances on removal, got ${result.version}`);
    },
  },
  {
    id: 'idempotent-add-replay',
    goal: 'Retry an append with the same key and confirm it is not duplicated.',
    async run({ client }) {
      const changeSet = await createChangeSet(client, 'eval-replay-create');
      const params = {
        changeSetId: changeSet.id,
        expectedVersion: changeSet.version,
        idempotencyKey: 'eval-replay-add',
        entries: [{
          source: { kind: 'agent', label: 'eval' },
          operation: boxNode('node-900005', 'Replay Once'),
        }],
      };
      const first = await client.call('changeset.addOperations', params);
      const second = await client.call('changeset.addOperations', params);
      return { first, second };
    },
    check(result, { assert }) {
      assert(result.first.version === 1, 'first append lands');
      assert(result.second.version === 1, `a replay must not bump the version, got ${result.second.version}`);
      assert(result.second.entries.length === 1, 'a replay must not duplicate the entry');
    },
  },
  {
    id: 'idempotency-key-reuse',
    goal: 'Reusing a key for a different payload must be refused.',
    async run({ client }) {
      const changeSet = await createChangeSet(client, 'eval-reuse-create');
      const base = {
        changeSetId: changeSet.id,
        expectedVersion: changeSet.version,
        idempotencyKey: 'eval-reuse-add',
      };
      await client.call('changeset.addOperations', {
        ...base,
        entries: [{
          source: { kind: 'agent', label: 'eval' },
          operation: boxNode('node-900006', 'First Payload'),
        }],
      });
      return expectFailure(() => client.call('changeset.addOperations', {
        ...base,
        entries: [{
          source: { kind: 'agent', label: 'eval' },
          operation: boxNode('node-900007', 'Different Payload'),
        }],
      }));
    },
    check(result, { assert }) {
      assert(result.failed, 'the second payload must be rejected');
      assert(
        String(result.message).toLowerCase().includes('idempotency'),
        `unexpected message: ${result.message}`,
      );
    },
  },
  {
    id: 'version-conflict',
    goal: 'A stale expectedVersion must be rejected instead of silently applied.',
    async run({ client }) {
      const changeSet = await createChangeSet(client, 'eval-conflict-create');
      await client.call('changeset.addOperations', {
        changeSetId: changeSet.id,
        expectedVersion: changeSet.version,
        idempotencyKey: 'eval-conflict-add-1',
        entries: [{
          source: { kind: 'agent', label: 'eval' },
          operation: boxNode('node-900008', 'First'),
        }],
      });
      return expectFailure(() => client.call('changeset.addOperations', {
        changeSetId: changeSet.id,
        expectedVersion: 0,
        idempotencyKey: 'eval-conflict-add-2',
        entries: [{
          source: { kind: 'agent', label: 'eval' },
          operation: boxNode('node-900009', 'Stale'),
        }],
      }));
    },
    check(result, { assert }) {
      assert(result.failed, 'a stale version must fail');
      assert(
        String(result.message).toLowerCase().includes('version'),
        `unexpected message: ${result.message}`,
      );
    },
  },
  {
    id: 'invalid-operation',
    goal: 'A malformed operation must come back as a structured diagnostic.',
    async run({ client }) {
      const changeSet = await createChangeSet(client, 'eval-invalid-create');
      const added = await client.call('changeset.addOperations', {
        changeSetId: changeSet.id,
        expectedVersion: changeSet.version,
        idempotencyKey: 'eval-invalid-add',
        entries: [{
          source: { kind: 'agent', label: 'eval' },
          operation: boxNode('not-a-node-id', 'Broken'),
        }],
      });
      // Appending does not evaluate, so the bad id only surfaces at evaluation.
      return client.call('changeset.evaluate', {
        changeSetId: changeSet.id,
        expectedVersion: added.version,
        includeCandidate: false,
      });
    },
    check(result, { assert }) {
      assert(result.status === 'blocked', `expected blocked, got ${result.status}`);
      assert(
        result.diagnostics.some((item) => item.code === 'ICE3_STUDIO_REFERENCE_INVALID'),
        `expected ICE3_STUDIO_REFERENCE_INVALID, got ${JSON.stringify(result.diagnostics)}`,
      );
      assert(result.candidateDigest === null, 'a blocked candidate has no digest');
    },
  },
  {
    id: 'commit-denied',
    goal: 'A proposal token must not be able to commit by itself.',
    async run({ client }) {
      return expectFailure(() => client.call('changeset.commit', {
        changeSetId: 'changeset-0000000000000000',
        expectedVersion: 0,
        approvedDigest: '0'.repeat(64),
        idempotencyKey: 'eval-commit',
      }));
    },
    check(result, { assert }) {
      assert(result.failed, 'commit must be refused for a proposal token');
      assert(
        /auth|permission|forbidden|authority/i.test(String(result.message)),
        `expected an authority error, got ${result.message}`,
      );
    },
  },
  {
    id: 'discard-proposal',
    goal: 'Discard a proposal and confirm the terminal state is readable.',
    async run({ client, approvalClient }) {
      const changeSet = await createChangeSet(client, 'eval-discard-create');
      const approver = await approvalClient();
      await approver.call('changeset.discard', {
        changeSetId: changeSet.id,
        idempotencyKey: 'eval-discard',
      });
      return client.call('changeset.get', { changeSetId: changeSet.id });
    },
    check(result, { assert }) {
      assert(result.status === 'discarded', `expected discarded, got ${result.status}`);
      assert(result.entries.length === 0, 'a discarded proposal keeps its (empty) entries');
    },
  },
];
