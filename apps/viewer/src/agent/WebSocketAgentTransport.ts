import {
  parseAgentServerMessage,
  serializeAgentClientMessage,
} from './agentProtocol';
import type {
  AgentConnection,
  AgentEvent,
  AgentIntent,
  AgentTransport,
} from './transport';

const DEFAULT_CAMERA = {
  position: [0, 0, 1] as [number, number, number],
  target: [0, 0, 0] as [number, number, number],
  fov: 42,
};

export function createWebSocketAgentTransport(options: {
  token: string;
  WebSocketImpl?: typeof WebSocket;
}): AgentTransport {
  const WebSocketImpl = options.WebSocketImpl ?? WebSocket;
  const listeners = new Set<(event: AgentEvent) => void>();
  const pending: string[] = [];
  let socket: WebSocket | null = null;
  let connection: AgentConnection = 'connecting';

  const emit = (event: AgentEvent): void => {
    for (const listener of listeners) listener(event);
  };
  const updateConnection = (next: AgentConnection): void => {
    if (connection === next) return;
    connection = next;
    emit({ type: 'connection.updated', connection: next });
  };
  const ensureSocket = (): WebSocket => {
    if (socket) return socket;
    const protocol = globalThis.location?.protocol === 'https:' ? 'wss:' : 'ws:';
    const host = globalThis.location?.host ?? '127.0.0.1';
    updateConnection('connecting');
    socket = new WebSocketImpl(
      `${protocol}//${host}/agent/ws?token=${encodeURIComponent(options.token)}`,
    );
    socket.addEventListener('open', () => {
      updateConnection('connected');
      for (const message of pending.splice(0)) socket?.send(message);
    });
    socket.addEventListener('message', (event) => {
      try {
        const parsed = parseAgentServerMessage(JSON.parse(String(event.data)));
        if (!parsed) return;
        if (parsed.type === 'gateway.unavailable') updateConnection('unavailable');
        emit(parsed);
      } catch {
        updateConnection('error');
      }
    });
    socket.addEventListener('error', () => updateConnection('error'));
    socket.addEventListener('close', () => {
      socket = null;
      if (connection !== 'error' && connection !== 'unavailable') {
        updateConnection('unavailable');
      }
      emit({
        type: 'gateway.unavailable',
        protocolVersion: 1,
        message: 'Agent Gateway connection closed',
      });
    });
    return socket;
  };
  const sendOrQueue = (message: string): void => {
    const current = ensureSocket();
    if (current.readyState === WebSocketImpl.OPEN) current.send(message);
    else {
      pending.push(message);
      if (pending.length > 256) pending.shift();
    }
  };

  return {
    get connection() {
      return connection;
    },
    connect() {
      if (
        socket
        && (socket.readyState === WebSocketImpl.CLOSING || socket.readyState === WebSocketImpl.CLOSED)
      ) {
        socket = null;
      }
      ensureSocket();
    },
    async submitIntent(intent) {
      const requestId = crypto.randomUUID();
      sendOrQueue(serializeAgentClientMessage({
        type: 'intent.submit',
        protocolVersion: 1,
        requestId,
        intent: {
          ...intent,
          focusNodeId: intent.focusNodeId ?? null,
          camera: intent.camera ?? DEFAULT_CAMERA,
        },
      }));
      return { requestId };
    },
    async cancel(runId) {
      sendOrQueue(serializeAgentClientMessage({
        type: 'run.cancel',
        protocolVersion: 1,
        runId,
      }));
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}
