import assert from 'node:assert/strict';
import test from 'node:test';
import { AgentStore } from '../src/agent/AgentStore';
import { createDeterministicTransport } from '../src/agent/deterministicTransport';

test('Agent failure leaves model review available', async () => {
  const store = new AgentStore({
    connection: 'unavailable',
    submitIntent: async () => ({ runId: 'run-1' }),
    cancel: async () => {},
    subscribe: () => () => {},
  });
  await store.submit('Resize the selected assembly');
  assert.equal(store.snapshot.connection, 'unavailable');
  assert.equal(store.snapshot.reviewAvailable, true);
});

test('deterministic transport emits a linked candidate plan', async () => {
  const events: string[] = [];
  const transport = createDeterministicTransport({
    plan: 'Stage a generic node update',
    changeSetId: 'changeset-test',
  });
  const unsubscribe = transport.subscribe((event) => events.push(event.type));
  await transport.submitIntent({
    text: 'Adjust the assembly',
    projectId: 'generic-project',
    revision: 3,
    selectionIds: ['node-000001'],
  });
  await new Promise((resolve) => setTimeout(resolve, 0));
  unsubscribe();
  assert.ok(events.includes('plan.updated'));
  assert.ok(events.includes('changeset.linked'));
});
