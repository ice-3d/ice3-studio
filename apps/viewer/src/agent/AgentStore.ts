import type { AgentConnection, AgentTransport } from './transport';
import { ExternalStore } from '../shared/ExternalStore';

export type AgentSnapshot = {
  connection: AgentConnection;
  requestId: string | null;
  runId: string | null;
  messages: string[];
  plan: string | null;
  linkedChangeSetId: string | null;
  reviewAvailable: true;
  error: string | null;
};

export class AgentStore extends ExternalStore<AgentSnapshot> {
  constructor(private readonly transport: AgentTransport) {
    super({
      connection: transport.connection,
      requestId: null,
      runId: null,
      messages: [],
      plan: null,
      linkedChangeSetId: null,
      reviewAvailable: true,
      error: null,
    });
    transport.subscribe((event) => {
      if (event.type === 'message.delta') {
        this.update({ ...this.snapshot, messages: [...this.snapshot.messages, event.text] });
      } else if (event.type === 'plan.updated') {
        this.update({ ...this.snapshot, plan: event.summary });
      } else if (event.type === 'changeset.linked') {
        this.update({
          ...this.snapshot,
          linkedChangeSetId: event.changeSetId,
        });
      } else if (event.type === 'run.failed') {
        this.update({ ...this.snapshot, error: event.message });
      } else if (event.type === 'run.started') {
        this.update({
          ...this.snapshot,
          requestId: event.requestId,
          runId: event.runId,
        });
      } else if (event.type === 'connection.updated') {
        this.update({ ...this.snapshot, connection: event.connection });
      } else if (event.type === 'gateway.unavailable') {
        this.update({
          ...this.snapshot,
          connection: 'unavailable',
          error: event.message,
        });
      }
    });
  }

  async submit(text: string): Promise<void> {
    if (this.snapshot.connection === 'unavailable') return;
    const run = await this.transport.submitIntent({
      text,
      projectId: '',
      revision: 0,
      selectionIds: [],
    });
    this.update({ ...this.snapshot, requestId: run.requestId });
  }
}
