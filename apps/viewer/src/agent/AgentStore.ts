import type { AgentTransport } from './transport';
import { ExternalStore } from '../shared/ExternalStore';

export type AgentSnapshot = {
  connection: 'connected' | 'unavailable';
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
        this.update({ ...this.snapshot, linkedChangeSetId: event.changeSetId });
      } else if (event.type === 'run.failed') {
        this.update({ ...this.snapshot, error: event.message });
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
    this.update({ ...this.snapshot, runId: run.runId });
  }
}
