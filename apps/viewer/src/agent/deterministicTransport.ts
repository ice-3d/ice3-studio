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
    async submitIntent() {
      const runId = 'deterministic-run';
      queueMicrotask(() => {
        emit({ type: 'run.started', runId });
        emit({ type: 'plan.updated', runId, summary: options.plan });
        emit({ type: 'changeset.linked', runId, changeSetId: options.changeSetId });
        emit({ type: 'run.finished', runId });
      });
      return { runId };
    },
    async cancel() {},
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}
