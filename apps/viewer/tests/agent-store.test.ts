import assert from 'node:assert/strict';
import test from 'node:test';
import { AgentStore } from '../src/agent/AgentStore';

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
