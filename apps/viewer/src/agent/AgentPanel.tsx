import { useState, useSyncExternalStore } from 'react';
import { AgentStore } from './AgentStore';

const unavailableStore = new AgentStore({
  connection: 'unavailable',
  submitIntent: async () => ({ runId: '' }),
  cancel: async () => {},
  subscribe: () => () => {},
});

type AgentPanelProps = {
  store?: AgentStore;
  projectId?: string;
  revision?: number;
  focusName?: string | null;
  onOpenManual?: () => void;
};

export function AgentPanel({
  store = unavailableStore,
  projectId,
  revision,
  focusName,
  onOpenManual,
}: AgentPanelProps) {
  const [text, setText] = useState('');
  const snapshot = useSyncExternalStore(store.subscribe, () => store.snapshot);
  const connected = snapshot.connection === 'connected';
  return (
    <section className="agent-panel" aria-label="Agent mission control">
      <header className="agent-panel__header">
        <div>
          <span className="panel-kicker">AI mission control</span>
          <h2>Agent</h2>
        </div>
        <span
          className={`agent-connection${connected ? ' agent-connection--ready' : ''}`}
          aria-live="polite"
        >
          <i aria-hidden="true" />
          {connected ? 'Ready' : 'Offline'}
        </span>
      </header>
      <p className="agent-panel__intro">
        Set the outcome. Agent uses the current project and selection as working context.
      </p>
      <div className="agent-context">
        <span>Working context</span>
        <dl>
          <div>
            <dt>Project</dt>
            <dd>{projectId || 'Loading'}</dd>
          </div>
          <div>
            <dt>Revision</dt>
            <dd>{revision ?? '—'}</dd>
          </div>
          <div>
            <dt>Focus</dt>
            <dd>{focusName ?? 'Whole model'}</dd>
          </div>
        </dl>
      </div>
      {!connected ? (
        <div className="agent-offline">
          <div className="agent-offline__heading">
            <span className="status-dot status-dot--error" aria-hidden="true" />
            <div>
              <strong>Agent Gateway is offline</strong>
              <span>New planning and modeling runs are paused.</span>
            </div>
          </div>
          <div className="agent-next-step">
            <span>Human path</span>
            <p>Inspect the focused object or review staged changes without leaving the workbench.</p>
          </div>
          <button type="button" className="button button--secondary" onClick={onOpenManual}>
            {focusName ? 'Inspect selected object' : 'Open manual controls'}
          </button>
        </div>
      ) : (
        <>
          <form className="agent-composer" onSubmit={(event) => {
            event.preventDefault();
            void store.submit(text).then(() => setText(''));
          }}>
            <label className="field">
              <span>Run objective</span>
              <textarea
                value={text}
                rows={5}
                placeholder="Describe the outcome, constraints, and what should stay unchanged..."
                onChange={(event) => setText(event.target.value)}
              />
            </label>
            <div className="agent-composer__actions">
              <span>Uses project, revision, and focus</span>
              <button type="submit" className="button button--primary" disabled={!text.trim()}>
                Run with Agent
              </button>
            </div>
          </form>
          {snapshot.error ? <p className="form-error" role="alert">{snapshot.error}</p> : null}
          {snapshot.plan ? (
            <section className="agent-plan" aria-label="Latest Agent plan">
              <span>Latest plan</span>
              <p>{snapshot.plan}</p>
            </section>
          ) : null}
          {snapshot.messages.length > 0 ? (
            <section className="agent-activity" aria-label="Agent activity">
              <span>Activity</span>
              {snapshot.messages.map((message, index) => (
                <p key={`${index}-${message}`}>{message}</p>
              ))}
            </section>
          ) : null}
        </>
      )}
    </section>
  );
}
