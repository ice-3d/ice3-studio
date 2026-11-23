import type {
  AgentConnection,
  AgentContext,
  AgentEvent,
  AgentToolPhase,
  AgentTransport,
} from './transport';
import { ExternalStore } from '../shared/ExternalStore';

const MAX_MESSAGE_BYTES = 64 * 1024;
const MAX_TOOLS = 100;

export type AgentRunStatus =
  | 'idle'
  | 'starting'
  | 'running'
  | 'finished'
  | 'failed'
  | 'cancelled';

export type AgentToolStatus = {
  key: string;
  runId: string;
  toolName: string;
  phase: AgentToolPhase;
  summary: string;
};

export type AgentSnapshot = {
  connection: AgentConnection;
  runStatus: AgentRunStatus;
  requestId: string | null;
  runId: string | null;
  messages: string[];
  plan: string | null;
  tools: AgentToolStatus[];
  linkedChangeSetId: string | null;
  linkedEntryIds: string[];
  lastContext: AgentContext | null;
  error: string | null;
  reviewAvailable: true;
  truncated: boolean;
};

export type AgentSubmission = {
  text: string;
  context: AgentContext;
};

export class AgentStore extends ExternalStore<AgentSnapshot> {
  private lastSubmission: AgentSubmission | null = null;
  private retryAfterConnect = false;

  constructor(private readonly transport: AgentTransport) {
    super({
      connection: transport.connection,
      runStatus: 'idle',
      requestId: null,
      runId: null,
      messages: [],
      plan: null,
      tools: [],
      linkedChangeSetId: null,
      linkedEntryIds: [],
      lastContext: null,
      error: null,
      reviewAvailable: true,
      truncated: false,
    });
    transport.subscribe((event) => this.applyEvent(event));
    transport.connect();
  }

  async submit(input: AgentSubmission): Promise<void> {
    if (this.snapshot.connection !== 'connected') {
      this.retryAfterConnect = false;
      const message = 'Agent Gateway is unavailable';
      this.update({
        ...this.snapshot,
        error: message,
      });
      throw new Error(message);
    }
    this.lastSubmission = structuredClone(input);
    this.update({
      ...this.snapshot,
      runStatus: 'starting',
      requestId: null,
      runId: null,
      messages: [],
      plan: null,
      tools: [],
      linkedChangeSetId: null,
      linkedEntryIds: [],
      lastContext: structuredClone(input.context),
      error: null,
      truncated: false,
    });
    try {
      const { requestId } = await this.transport.submitIntent({
        text: input.text,
        ...input.context,
      });
      this.update({ ...this.snapshot, requestId });
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      this.update({
        ...this.snapshot,
        runStatus: 'failed',
        error: message,
      });
      throw new Error(message);
    }
  }

  async cancel(): Promise<void> {
    const runId = this.snapshot.runId;
    if (!runId) return;
    await this.transport.cancel(runId);
    this.update({
      ...this.snapshot,
      runStatus: 'cancelled',
      error: null,
    });
  }

  async retry(): Promise<void> {
    if (this.snapshot.connection !== 'connected') {
      this.retryAfterConnect = this.lastSubmission !== null;
      this.transport.connect();
      return;
    }
    if (!this.lastSubmission) return;
    try {
      await this.submit(this.lastSubmission);
    } catch {
      // The snapshot carries the user-facing error.
    }
  }

  dismissError(): void {
    this.update({ ...this.snapshot, error: null });
  }

  private applyEvent(event: AgentEvent): void {
    if (event.type === 'connection.updated') {
      this.update({ ...this.snapshot, connection: event.connection });
      if (event.connection === 'connected' && this.retryAfterConnect) {
        this.retryAfterConnect = false;
        if (this.lastSubmission) {
          void this.submit(this.lastSubmission).catch(() => {});
        }
      }
      return;
    }
    if (event.type === 'gateway.unavailable') {
      const interrupted = this.snapshot.runStatus === 'starting'
        || this.snapshot.runStatus === 'running';
      this.update({
        ...this.snapshot,
        connection: 'unavailable',
        runStatus: interrupted ? 'failed' : this.snapshot.runStatus,
        error: event.message,
      });
      return;
    }
    if (event.type === 'pong') return;
    if (event.type === 'run.started') {
      if (event.requestId !== this.snapshot.requestId || this.snapshot.runId) return;
      this.update({
        ...this.snapshot,
        runId: event.runId,
        runStatus: 'running',
      });
      return;
    }
    if (event.runId !== this.snapshot.runId || this.snapshot.runStatus === 'cancelled') {
      return;
    }
    switch (event.type) {
      case 'message.delta':
        this.appendMessage(event.text);
        break;
      case 'plan.updated':
        this.update({ ...this.snapshot, plan: event.summary });
        break;
      case 'tool.updated':
        this.appendTool(event);
        break;
      case 'changeset.linked':
        this.update({
          ...this.snapshot,
          linkedChangeSetId: event.changeSetId,
          linkedEntryIds: [...event.entryIds],
        });
        break;
      case 'run.finished':
        this.update({ ...this.snapshot, runStatus: 'finished' });
        break;
      case 'run.failed':
        this.update({
          ...this.snapshot,
          runStatus: 'failed',
          error: event.message,
        });
        break;
    }
  }

  private appendMessage(text: string): void {
    let messages = [...this.snapshot.messages, text];
    let truncated = this.snapshot.truncated;
    let total = messages.reduce((sum, message) => sum + message.length, 0);
    while (total > MAX_MESSAGE_BYTES && messages.length > 1) {
      total -= messages[0]!.length;
      messages = messages.slice(1);
      truncated = true;
    }
    if (total > MAX_MESSAGE_BYTES && messages[0]) {
      messages[0] = messages[0].slice(-MAX_MESSAGE_BYTES);
      truncated = true;
    }
    this.update({ ...this.snapshot, messages, truncated });
  }

  private appendTool(event: Extract<AgentEvent, { type: 'tool.updated' }>): void {
    const tool: AgentToolStatus = {
      key: `${event.runId}:${this.snapshot.tools.length}:${event.toolName}`,
      runId: event.runId,
      toolName: event.toolName,
      phase: event.phase,
      summary: event.summary,
    };
    this.update({
      ...this.snapshot,
      tools: [...this.snapshot.tools, tool].slice(-MAX_TOOLS),
    });
  }
}
