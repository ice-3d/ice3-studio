import { useRef, useState, useSyncExternalStore } from 'react';
import { AgentStore, type AgentRunStatus } from './AgentStore';

const unavailableStore = new AgentStore({
  connection: 'unavailable',
  connect: () => {},
  submitIntent: async () => ({ requestId: '' }),
  cancel: async () => {},
  subscribe: () => () => {},
});

type AgentPanelProps = {
  store?: AgentStore;
  projectId?: string;
  revision?: number;
  focusName?: string | null;
  onOpenManual?: () => void;
  onSubmit?: (text: string) => Promise<void> | void;
  onCancel?: () => Promise<void> | void;
  onRetry?: () => Promise<void> | void;
  onReviewChanges?: () => void;
};

export function AgentPanel({
  store = unavailableStore,
  projectId,
  revision,
  focusName,
  onOpenManual,
  onSubmit,
  onCancel,
  onRetry,
  onReviewChanges,
}: AgentPanelProps) {
  const [text, setText] = useState('');
  const [submitError, setSubmitError] = useState<string | null>(null);
  const form = useRef<HTMLFormElement>(null);
  const snapshot = useSyncExternalStore(store.subscribe, () => store.snapshot);
  const connected = snapshot.connection === 'connected';
  const running = snapshot.runStatus === 'starting' || snapshot.runStatus === 'running';
  const status = connectionLabel(snapshot.connection, snapshot.runStatus);
  const submit = (): void => {
    if (!text.trim() || running || !connected) return;
    setSubmitError(null);
    void Promise.resolve(onSubmit?.(text))
      .then(() => setText(''))
      .catch((cause) => {
        setSubmitError(cause instanceof Error ? cause.message : String(cause));
      });
  };

  return (
    <section className="agent-panel" aria-label="Agent mission control">
      <header className="agent-panel__header">
        <div>
          <span className="panel-kicker">AI mission control</span>
          <h2>Agent</h2>
        </div>
        <span
          className={`agent-connection agent-connection--${snapshot.connection}`}
          aria-live="polite"
        >
          <i aria-hidden="true" />
          {status}
        </span>
      </header>

      <div className="agent-context-strip" aria-label="Agent working context">
        <ContextChip label="Project" value={projectId || 'Loading'} />
        <ContextChip label="Revision" value={String(revision ?? '—')} />
        <ContextChip label="Focus" value={focusName ?? 'Whole model'} wide />
      </div>

      {snapshot.connection === 'connecting' ? (
        <div className="agent-connecting" role="status">
          <span className="status-dot" aria-hidden="true" />
          <div>
            <strong>Connecting to Agent Gateway</strong>
            <span>Manual tools remain available while the session opens.</span>
          </div>
        </div>
      ) : null}

      {connected ? (
        <form
          ref={form}
          className="agent-mission"
          aria-label="Agent objective"
          onSubmit={(event) => {
            event.preventDefault();
            submit();
          }}
        >
          <label className="agent-objective">
            <span>What should change?</span>
            <textarea
              aria-label="Run objective"
              value={text}
              rows={5}
              disabled={running}
              placeholder="Describe the outcome, constraints, and what must remain unchanged..."
              onChange={(event) => setText(event.target.value)}
              onKeyDown={(event) => {
                if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') {
                  event.preventDefault();
                  form.current?.requestSubmit();
                }
              }}
            />
            <small>
              {navigator.platform.includes('Mac') ? '⌘' : 'Ctrl'} Enter to run · uses project,
              revision and selection
            </small>
          </label>
          <button
            type="submit"
            className="button button--primary"
            disabled={!text.trim() || running}
          >
            {running ? 'Agent is working…' : 'Run with Agent'}
          </button>
          {submitError ? <p className="form-error" role="alert">{submitError}</p> : null}
        </form>
      ) : null}

      {snapshot.connection === 'unavailable' || snapshot.connection === 'error' ? (
        <div className="agent-offline">
          <div className="agent-offline__heading">
            <span className="status-dot status-dot--error" aria-hidden="true" />
            <div>
              <strong>
                {snapshot.connection === 'unavailable'
                  ? 'Agent Gateway is offline'
                  : 'Agent Gateway needs attention'}
              </strong>
              <span>
                {snapshot.error || 'New planning and modeling runs are paused.'}
              </span>
            </div>
          </div>
          <div className="agent-next-step">
            <span>Configure</span>
            <code>STUDIO_AGENT_GATEWAY_URL</code>
            <p>Set a loopback WebSocket URL and restart Studio to enable Agent runs.</p>
          </div>
          <div className="agent-offline__actions">
            {snapshot.connection === 'error' && onRetry ? (
              <button
                type="button"
                className="button button--secondary"
                onClick={() => void onRetry()}
              >
                Retry
              </button>
            ) : null}
            <button
              type="button"
              className="button button--secondary"
              onClick={onOpenManual}
            >
              {focusName ? 'Inspect selected object' : 'Open manual controls'}
            </button>
          </div>
        </div>
      ) : null}

      {snapshot.runStatus !== 'idle' ? (
        <section className={`agent-run agent-run--${snapshot.runStatus}`} aria-label="Agent run">
          <div className="agent-run__heading">
            <div>
              <span className="panel-kicker">Run</span>
              <strong>{runLabel(snapshot.runStatus)}</strong>
            </div>
            {running ? (
              <button
                type="button"
                className="button button--secondary"
                onClick={() => void onCancel?.()}
              >
                Cancel
              </button>
            ) : null}
          </div>
          <small>{snapshot.runId || snapshot.requestId || 'Waiting for Gateway'}</small>
        </section>
      ) : null}

      {snapshot.plan ? (
        <section className="agent-plan" aria-label="Latest Agent plan">
          <span>Plan</span>
          <p>{snapshot.plan}</p>
        </section>
      ) : null}

      {snapshot.tools.length > 0 ? (
        <section className="agent-tool-section" aria-label="Agent tool activity">
          <span>Tool activity</span>
          <ol className="agent-tools">
            {snapshot.tools.map((tool) => (
              <li className={`agent-tool agent-tool--${tool.phase}`} key={tool.key}>
                <code>{tool.toolName}</code>
                <span>{tool.phase}</span>
                <small>{tool.summary}</small>
              </li>
            ))}
          </ol>
        </section>
      ) : null}

      {snapshot.messages.length > 0 ? (
        <details className="agent-activity" open={running}>
          <summary>
            Activity
            {snapshot.truncated ? <span>truncated</span> : null}
          </summary>
          <p>{snapshot.messages.join('')}</p>
        </details>
      ) : null}

      {snapshot.error && snapshot.runStatus === 'failed' ? (
        <div className="agent-run-error" role="alert">
          <strong>Run failed</strong>
          <span>{snapshot.error}</span>
          {onRetry ? (
            <button
              type="button"
              className="button button--secondary"
              onClick={() => void onRetry()}
            >
              Retry
            </button>
          ) : null}
        </div>
      ) : null}

      {snapshot.linkedChangeSetId ? (
        <section className="agent-handoff" aria-label="Linked ChangeSet">
          <div>
            <span>Ready for review</span>
            <strong>{snapshot.linkedChangeSetId}</strong>
          </div>
          <button
            type="button"
            className="button button--primary"
            onClick={onReviewChanges}
          >
            Review changes
          </button>
        </section>
      ) : null}
    </section>
  );
}

function ContextChip({
  label,
  value,
  wide = false,
}: {
  label: string;
  value: string;
  wide?: boolean;
}) {
  return (
    <span className={`agent-context-chip${wide ? ' agent-context-chip--wide' : ''}`}>
      <span>{label}</span>
      <strong>{value}</strong>
    </span>
  );
}

function connectionLabel(
  connection: 'connecting' | 'connected' | 'unavailable' | 'error',
  run: AgentRunStatus,
): string {
  if (connection === 'connecting') return 'Connecting';
  if (connection === 'unavailable') return 'Offline';
  if (connection === 'error') return 'Error';
  return run === 'idle' ? 'Ready' : runLabel(run);
}

function runLabel(status: AgentRunStatus): string {
  return status.charAt(0).toUpperCase() + status.slice(1);
}
