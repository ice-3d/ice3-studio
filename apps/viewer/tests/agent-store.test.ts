import assert from 'node:assert/strict';
import test from 'node:test';
import { AgentStore } from '../src/agent/AgentStore';
import { createDeterministicTransport } from '../src/agent/deterministicTransport';
import type {
  AgentConnection,
  AgentEvent,
  AgentIntent,
  AgentTransport,
} from '../src/agent/transport';

const context = {
  projectId: 'plant',
  revision: 7,
  selectionIds: ['node-7'],
  focusNodeId: 'node-7',
  camera: {
    position: [1, 2, 3] as [number, number, number],
    target: [0, 0, 0] as [number, number, number],
    fov: 42,
  },
};

function manualTransport(options: {
  connection?: AgentConnection;
  requestId?: string;
  submitIntent?: AgentTransport['submitIntent'];
} = {}): AgentTransport & {
  emit(event: AgentEvent): void;
  cancelled: string[];
  connectCount: number;
} {
  const listeners = new Set<(event: AgentEvent) => void>();
  return {
    connection: options.connection ?? 'connected',
    connectCount: 0,
    connect() {
      this.connectCount += 1;
    },
    cancelled: [],
    submitIntent: options.submitIntent ?? (async () => ({
      requestId: options.requestId ?? 'request-1',
    })),
    async cancel(runId) {
      this.cancelled.push(runId);
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    emit(event) {
      for (const listener of listeners) listener(event);
    },
  };
}

test('submits the latest context instead of empty defaults', async () => {
  let received: AgentIntent | undefined;
  const transport = manualTransport({
    submitIntent: async (intent) => {
      received = intent;
      return { requestId: 'request-1' };
    },
  });
  const store = new AgentStore(transport);
  await store.submit({ text: 'Move the pump', context });
  assert.deepEqual(received, {
    text: 'Move the pump',
    ...context,
  });
  assert.equal(store.snapshot.runStatus, 'starting');
  assert.equal(store.snapshot.requestId, 'request-1');
});

test('ignores events from a previous run', async () => {
  const transport = manualTransport();
  const store = new AgentStore(transport);
  await store.submit({ text: 'Move it', context });
  transport.emit({
    type: 'run.started',
    protocolVersion: 1,
    requestId: 'request-1',
    runId: 'run-1',
  });
  transport.emit({
    type: 'plan.updated',
    protocolVersion: 1,
    runId: 'run-old',
    summary: 'Old plan',
  });
  assert.equal(store.snapshot.plan, null);
  assert.equal(store.snapshot.runStatus, 'running');
});

test('cancels the active run and ignores late events', async () => {
  const transport = manualTransport();
  const store = new AgentStore(transport);
  await store.submit({ text: 'Move it', context });
  transport.emit({
    type: 'run.started',
    protocolVersion: 1,
    requestId: 'request-1',
    runId: 'run-1',
  });
  await store.cancel();
  assert.deepEqual(transport.cancelled, ['run-1']);
  assert.equal(store.snapshot.runStatus, 'cancelled');
  transport.emit({ type: 'run.finished', protocolVersion: 1, runId: 'run-1' });
  assert.equal(store.snapshot.runStatus, 'cancelled');
});

test('does not send an empty run id while the run is starting', async () => {
  const transport = manualTransport();
  const store = new AgentStore(transport);
  await store.submit({ text: 'Move it', context });
  assert.equal(store.snapshot.runStatus, 'starting');
  assert.equal(store.snapshot.runId, null);

  await store.cancel();

  assert.deepEqual(transport.cancelled, []);
  assert.equal(store.snapshot.runStatus, 'starting');
});

test('rejects an unavailable submission so the objective remains editable', async () => {
  const transport = manualTransport({ connection: 'unavailable' });
  const store = new AgentStore(transport);

  await assert.rejects(
    () => store.submit({ text: 'Keep this objective', context }),
    /unavailable/,
  );
  assert.equal(store.snapshot.runStatus, 'idle');
  assert.equal(store.snapshot.error, 'Agent Gateway is unavailable');
});

test('retries the connection when the Gateway is not connected', async () => {
  const transport = manualTransport({ connection: 'error' });
  const store = new AgentStore(transport);
  const initialConnects = transport.connectCount;

  await store.retry();

  assert.equal(transport.connectCount, initialConnects + 1);
  assert.equal(store.snapshot.runStatus, 'idle');
});

test('resubmits the last objective after reconnecting', async () => {
  let submissions = 0;
  const transport = manualTransport({
    submitIntent: async () => ({ requestId: `request-${++submissions}` }),
  });
  const store = new AgentStore(transport);
  await store.submit({ text: 'Move it', context });
  transport.emit({
    type: 'gateway.unavailable',
    protocolVersion: 1,
    message: 'Agent Gateway connection closed',
  });

  await store.retry();
  transport.emit({ type: 'connection.updated', connection: 'connected' });
  await new Promise((resolve) => setTimeout(resolve, 0));

  assert.equal(submissions, 2);
  assert.equal(store.snapshot.requestId, 'request-2');
});

test('bounds streamed messages and tool status', async () => {
  const transport = manualTransport();
  const store = new AgentStore(transport);
  await store.submit({ text: 'Move it', context });
  transport.emit({
    type: 'run.started',
    protocolVersion: 1,
    requestId: 'request-1',
    runId: 'run-1',
  });
  for (let index = 0; index < 100; index += 1) {
    transport.emit({
      type: 'message.delta',
      protocolVersion: 1,
      runId: 'run-1',
      text: 'x'.repeat(1024),
    });
    transport.emit({
      type: 'tool.updated',
      protocolVersion: 1,
      runId: 'run-1',
      toolName: `tool-${index}`,
      phase: 'finished',
      summary: 'ok',
    });
  }
  assert.equal(store.snapshot.truncated, true);
  assert.ok(store.snapshot.messages.join('').length <= 64 * 1024);
  assert.equal(store.snapshot.tools.length, 100);
});

test('links a ChangeSet and records failed runs', async () => {
  const transport = manualTransport();
  const store = new AgentStore(transport);
  await store.submit({ text: 'Move it', context });
  transport.emit({
    type: 'run.started',
    protocolVersion: 1,
    requestId: 'request-1',
    runId: 'run-1',
  });
  transport.emit({
    type: 'changeset.linked',
    protocolVersion: 1,
    runId: 'run-1',
    changeSetId: 'changeset-1',
    entryIds: ['entry-1'],
  });
  assert.equal(store.snapshot.linkedChangeSetId, 'changeset-1');
  assert.deepEqual(store.snapshot.linkedEntryIds, ['entry-1']);
  transport.emit({
    type: 'run.failed',
    protocolVersion: 1,
    runId: 'run-1',
    diagnosticCode: 'ICE3_AGENT_RUN_FAILED',
    message: 'Evaluation failed',
  });
  assert.equal(store.snapshot.runStatus, 'failed');
  assert.equal(store.snapshot.error, 'Evaluation failed');
});

test('marks an active run as failed when the Gateway disconnects', async () => {
  const transport = manualTransport();
  const store = new AgentStore(transport);
  await store.submit({ text: 'Move it', context });
  transport.emit({
    type: 'run.started',
    protocolVersion: 1,
    requestId: 'request-1',
    runId: 'run-1',
  });

  transport.emit({
    type: 'gateway.unavailable',
    protocolVersion: 1,
    message: 'Agent Gateway connection closed',
  });

  assert.equal(store.snapshot.connection, 'unavailable');
  assert.equal(store.snapshot.runStatus, 'failed');
  assert.equal(store.snapshot.error, 'Agent Gateway connection closed');
});

test('retry resubmits the last unmapped context', async () => {
  let submissions = 0;
  const transport = manualTransport({
    submitIntent: async () => ({ requestId: `request-${++submissions}` }),
  });
  const store = new AgentStore(transport);
  await store.submit({ text: 'Resize the tank', context });
  transport.emit({
    type: 'run.started',
    protocolVersion: 1,
    requestId: 'request-1',
    runId: 'run-1',
  });
  transport.emit({
    type: 'run.failed',
    protocolVersion: 1,
    runId: 'run-1',
    diagnosticCode: 'ICE3_AGENT_RUN_FAILED',
    message: 'Retry me',
  });
  await store.retry();
  assert.equal(submissions, 2);
  assert.equal(store.snapshot.requestId, 'request-2');
  assert.equal(store.snapshot.error, null);
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
