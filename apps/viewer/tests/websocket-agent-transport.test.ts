import assert from 'node:assert/strict';
import test from 'node:test';
import { createWebSocketAgentTransport } from '../src/agent/WebSocketAgentTransport';
import type { AgentEvent } from '../src/agent/transport';

class FakeWebSocket {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSING = 2;
  static readonly CLOSED = 3;
  static instances: FakeWebSocket[] = [];
  readonly sent: string[] = [];
  readyState = FakeWebSocket.CONNECTING;
  private readonly listeners = new Map<string, Set<(event: MessageEvent) => void>>();

  constructor(readonly url: string) {
    FakeWebSocket.instances.push(this);
  }

  addEventListener(type: string, listener: EventListener): void {
    const listeners = this.listeners.get(type) ?? new Set();
    listeners.add(listener as (event: MessageEvent) => void);
    this.listeners.set(type, listeners);
  }

  send(data: string): void {
    this.sent.push(data);
  }

  close(): void {
    this.readyState = FakeWebSocket.CLOSED;
    this.emit('close', {} as MessageEvent);
  }

  open(): void {
    this.readyState = FakeWebSocket.OPEN;
    this.emit('open', {} as MessageEvent);
  }

  receive(value: unknown): void {
    this.emit('message', { data: JSON.stringify(value) } as MessageEvent);
  }

  private emit(type: string, event: MessageEvent): void {
    for (const listener of this.listeners.get(type) ?? []) listener(event);
  }
}

const implementation = FakeWebSocket as unknown as typeof WebSocket;

const context = {
  text: 'Move the pump',
  projectId: 'plant',
  revision: 4,
  selectionIds: ['node-1'],
  focusNodeId: 'node-1',
  camera: {
    position: [1, 2, 3] as [number, number, number],
    target: [0, 0, 0] as [number, number, number],
    fov: 42,
  },
};

test('queues intent until open and dispatches streamed events', async () => {
  FakeWebSocket.instances = [];
  const events: AgentEvent[] = [];
  const transport = createWebSocketAgentTransport({
    token: 'session-token',
    WebSocketImpl: implementation,
  });
  transport.subscribe((event) => events.push(event));
  const submitted = await transport.submitIntent(context);
  const socket = FakeWebSocket.instances[0]!;
  assert.equal(submitted.requestId.length > 0, true);
  assert.equal(socket.sent.length, 0);
  socket.open();
  assert.equal(socket.sent.length, 1);
  assert.match(socket.sent[0]!, /"projectId":"plant"/);
  assert.match(socket.url, /agent\/ws\?token=session-token/);

  socket.receive({
    type: 'run.started',
    protocolVersion: 1,
    requestId: submitted.requestId,
    runId: 'run-1',
  });
  socket.receive({
    type: 'tool.updated',
    protocolVersion: 1,
    runId: 'run-1',
    toolName: 'scene.search',
    phase: 'finished',
    summary: 'Found the pump',
  });
  socket.receive({ type: 'unknown', protocolVersion: 1 });
  assert.ok(events.some((event) => event.type === 'run.started'));
  assert.ok(events.some((event) => event.type === 'tool.updated'));
  assert.equal(events.some((event) => event.type === 'unknown' as never), false);
});

test('sends cancel and reports unavailable close state', async () => {
  FakeWebSocket.instances = [];
  const events: AgentEvent[] = [];
  const transport = createWebSocketAgentTransport({
    token: 'session-token',
    WebSocketImpl: implementation,
  });
  transport.subscribe((event) => events.push(event));
  await transport.submitIntent(context);
  const socket = FakeWebSocket.instances[0]!;
  socket.open();
  await transport.cancel('run-1');
  assert.match(socket.sent.at(-1)!, /"type":"run.cancel"/);
  socket.receive({
    type: 'gateway.unavailable',
    protocolVersion: 1,
    message: 'Gateway is offline',
  });
  assert.equal(transport.connection, 'unavailable');
  assert.ok(events.some((event) => event.type === 'gateway.unavailable'));
});

test('connects on startup to expose gateway availability', () => {
  FakeWebSocket.instances = [];
  const transport = createWebSocketAgentTransport({
    token: 'session-token',
    WebSocketImpl: implementation,
  });
  transport.connect();
  assert.equal(FakeWebSocket.instances.length, 1);
});
