import assert from 'node:assert/strict';
import test from 'node:test';
import {
  parseAgentServerMessage,
  serializeAgentClientMessage,
} from '../src/agent/agentProtocol';

test('serializes intent with the current context and no display-name fields', () => {
  const text = serializeAgentClientMessage({
    type: 'intent.submit',
    protocolVersion: 1,
    requestId: 'request-1',
    intent: {
      text: 'Move the pump',
      projectId: 'plant',
      revision: 3,
      selectionIds: ['node-1'],
      focusNodeId: 'node-1',
      camera: { position: [1, 2, 3], target: [0, 0, 0], fov: 42 },
    },
  });
  assert.match(text, /"projectId":"plant"/);
  assert.doesNotMatch(text, /focusName/);
});

test('parses known events and rejects unknown or wrong-version messages', () => {
  assert.deepEqual(
    parseAgentServerMessage({
      type: 'run.started',
      protocolVersion: 1,
      requestId: 'request-1',
      runId: 'run-1',
    }),
    {
      type: 'run.started',
      protocolVersion: 1,
      requestId: 'request-1',
      runId: 'run-1',
    },
  );
  assert.equal(parseAgentServerMessage({
    type: 'unknown',
    protocolVersion: 1,
  }), null);
  assert.equal(parseAgentServerMessage({
    type: 'run.started',
    protocolVersion: 2,
    requestId: 'request-1',
    runId: 'run-1',
  }), null);
});

test('parses tool progress and unavailable gateway messages', () => {
  assert.deepEqual(parseAgentServerMessage({
    type: 'tool.updated',
    protocolVersion: 1,
    runId: 'run-1',
    toolName: 'scene.search',
    phase: 'finished',
    summary: 'Found the pump',
  }), {
    type: 'tool.updated',
    protocolVersion: 1,
    runId: 'run-1',
    toolName: 'scene.search',
    phase: 'finished',
    summary: 'Found the pump',
  });
  assert.deepEqual(parseAgentServerMessage({
    type: 'gateway.unavailable',
    protocolVersion: 1,
    message: 'Gateway is offline',
  }), {
    type: 'gateway.unavailable',
    protocolVersion: 1,
    message: 'Gateway is offline',
  });
});
