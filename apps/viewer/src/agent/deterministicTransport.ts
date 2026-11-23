import type { AgentEvent, AgentTransport } from './transport';

export function createDeterministicTransport(options: {
  plan: string;
  changeSetId: string;
}): AgentTransport {
  const listeners = new Set<(event: AgentEvent) => void>();
  const emit = (event: AgentEvent) => {
    for (const listener of listeners) listener(event);
  };
  return {
    connection: 'connected',
    connect() {},
    async submitIntent() {
      const requestId = crypto.randomUUID();
      const runId = 'deterministic-run';
      queueMicrotask(() => {
        emit({ type: 'run.started', protocolVersion: 1, requestId, runId });
        emit({ type: 'plan.updated', protocolVersion: 1, runId, summary: options.plan });
        emit({
          type: 'changeset.linked',
          protocolVersion: 1,
          runId,
          changeSetId: options.changeSetId,
          entryIds: [],
        });
        emit({ type: 'run.finished', protocolVersion: 1, runId });
      });
      return { requestId };
    },
    async cancel() {},
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}
