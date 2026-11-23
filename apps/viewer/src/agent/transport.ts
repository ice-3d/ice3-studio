export type AgentConnection =
  | 'connecting'
  | 'connected'
  | 'unavailable'
  | 'error';

export type AgentToolPhase = 'started' | 'finished' | 'failed';

export type AgentCameraContext = {
  position: [number, number, number];
  target: [number, number, number];
  fov: number;
};

export type AgentContext = {
  projectId: string;
  revision: number;
  selectionIds: string[];
  focusNodeId: string | null;
  camera: AgentCameraContext;
};

export type AgentEvent =
  | { type: 'connection.updated'; connection: AgentConnection }
  | { type: 'gateway.unavailable'; protocolVersion: 1; message: string }
  | { type: 'run.started'; protocolVersion: 1; requestId: string; runId: string }
  | { type: 'message.delta'; protocolVersion: 1; runId: string; text: string }
  | { type: 'plan.updated'; protocolVersion: 1; runId: string; summary: string }
  | {
      type: 'tool.updated';
      protocolVersion: 1;
      runId: string;
      toolName: string;
      phase: AgentToolPhase;
      summary: string;
    }
  | {
      type: 'changeset.linked';
      protocolVersion: 1;
      runId: string;
      changeSetId: string;
      entryIds: string[];
    }
  | { type: 'run.finished'; protocolVersion: 1; runId: string }
  | {
      type: 'run.failed';
      protocolVersion: 1;
      runId: string;
      diagnosticCode: string;
      message: string;
    }
  | { type: 'pong'; protocolVersion: 1 };

export type AgentIntent = {
  text: string;
  projectId: string;
  revision: number;
  selectionIds: string[];
  focusNodeId?: string | null;
  camera?: AgentCameraContext;
};

export type AgentTransport = {
  readonly connection: AgentConnection;
  submitIntent(intent: AgentIntent): Promise<{ requestId: string }>;
  cancel(runId: string): Promise<void>;
  subscribe(listener: (event: AgentEvent) => void): () => void;
};
