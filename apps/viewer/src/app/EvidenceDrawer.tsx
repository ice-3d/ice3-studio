import type {
  ChangeSetEntry,
  Diagnostic,
  HistoryEntry,
  StudioNode,
} from '../api/StudioClient';
import { OperationList } from '../changeset/OperationList';
import { mergeDiagnostics } from '../changeset/reviewModel';

export type EvidenceTab = 'operations' | 'timeline' | 'diagnostics';

type EvidenceDrawerProps = {
  tab: EvidenceTab;
  history: readonly HistoryEntry[];
  diagnostics: readonly Diagnostic[];
  entries: readonly ChangeSetEntry[];
  nodes: readonly StudioNode[];
  onClose: () => void;
  onFocusDiagnostic: (objectId: string) => void;
};

export function EvidenceDrawer({
  tab,
  history,
  diagnostics,
  entries,
  nodes,
  onClose,
  onFocusDiagnostic,
}: EvidenceDrawerProps) {
  return (
    <section id="evidence-drawer" className="evidence-drawer" aria-label="Evidence drawer">
      <header className="evidence-drawer__header">
        <div>
          <span className="panel-kicker">Evidence</span>
          <h2>{evidenceTitle(tab)}</h2>
        </div>
        <button type="button" className="button button--quiet" onClick={onClose}>
          Close
        </button>
      </header>
      <div className="evidence-drawer__content">
        {tab === 'operations' && (
          <OperationList entries={entries} nodes={nodes} />
        )}
        {tab === 'timeline' && (
          <Timeline history={history} />
        )}
        {tab === 'diagnostics' && (
          <DiagnosticList diagnostics={diagnostics} onFocus={onFocusDiagnostic} />
        )}
      </div>
    </section>
  );
}

function Timeline({ history }: { history: readonly HistoryEntry[] }) {
  if (history.length === 0) {
    return (
      <div className="empty-state">
        <strong>No committed revisions</strong>
        <span>Timeline entries appear after the first persisted revision.</span>
      </div>
    );
  }
  return (
    <ol className="timeline-list" aria-label="Revision timeline">
      {[...history].reverse().map((entry) => {
        const kind = typeof entry.event.kind === 'string' ? entry.event.kind : 'revision';
        const operationCount = operationCountFromEvent(entry);
        return (
          <li key={`${entry.revision}:${entry.digest}`}>
            <span className="timeline-list__marker" aria-hidden="true" />
            <div>
              <strong>Revision {entry.revision}</strong>
              <span>{kind}</span>
            </div>
            <code>{operationCount === null ? shortDigest(entry.digest) : `${operationCount} ops`}</code>
          </li>
        );
      })}
    </ol>
  );
}

function DiagnosticList({
  diagnostics,
  onFocus,
}: {
  diagnostics: readonly Diagnostic[];
  onFocus: (objectId: string) => void;
}) {
  const unique = mergeDiagnostics(diagnostics);
  if (unique.length === 0) {
    return (
      <div className="empty-state">
        <strong>No diagnostics</strong>
        <span>The committed document and current candidate are clean.</span>
      </div>
    );
  }
  return (
    <ul className="diagnostic-list" aria-label="Diagnostics">
      {unique.map((diagnostic, index) => (
        <li key={`${diagnostic.code}:${diagnostic.operationIndex ?? ''}:${index}`}>
          <div>
            <code>{diagnostic.code}</code>
            <strong>{diagnostic.message}</strong>
            <span>{diagnostic.pointer || '/'}</span>
          </div>
          {diagnostic.objectId && (
            <button
              type="button"
              className="button button--quiet"
              onClick={() => onFocus(diagnostic.objectId!)}
            >
              Focus object
            </button>
          )}
        </li>
      ))}
    </ul>
  );
}

function evidenceTitle(tab: EvidenceTab): string {
  if (tab === 'operations') return 'Operations';
  if (tab === 'timeline') return 'Timeline';
  return 'Diagnostics';
}

function operationCountFromEvent(entry: HistoryEntry): number | null {
  const request = entry.event.request;
  if (typeof request === 'object' && request !== null && !Array.isArray(request)) {
    const operations = (request as Record<string, unknown>).operations;
    if (Array.isArray(operations)) return operations.length;
  }
  const count = entry.event.entryCount;
  return typeof count === 'number' ? count : null;
}

function shortDigest(digest: string): string {
  return digest.length > 12 ? digest.slice(0, 12) : digest;
}
