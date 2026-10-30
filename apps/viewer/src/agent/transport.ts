export type AgentEvent =
  | { type: 'run.started'; runId: string }
  | { type: 'message.delta'; runId: string; text: string }
  | { type: 'plan.updated'; runId: string; summary: string }
  | { type: 'changeset.linked'; runId: string; changeSetId: string }
  | { type: 'run.finished'; runId: string }
  | { type: 'run.failed'; runId: string; diagnosticCode: string; message: string };

export type AgentTransport = {
  connection: 'connected' | 'unavailable';
  submitIntent(intent: {
    text: string;
    projectId: string;
    revision: number;
    selectionIds: string[];
  }): Promise<{ runId: string }>;
  cancel(runId: string): Promise<void>;
  subscribe(listener: (event: AgentEvent) => void): () => void;
};
