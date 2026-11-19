import type {
  ChangeSetEntry,
  ChangeSetEvaluation,
  ChangeSetRecord,
  DocumentDiff,
  StudioNode,
} from '../api/StudioClient';
import type { ChangeSetViewStatus } from './ChangeSetStore';
import { OperationList } from './OperationList';
import { canMutateChangeSet, summarizeDiff } from './reviewModel';

type ChangesPanelProps = {
  active: ChangeSetEvaluation | null;
  record: ChangeSetRecord | null;
  status: ChangeSetViewStatus;
  nodes: readonly StudioNode[];
  error: string | null;
  onRemove: (entryId: string) => void;
};

export function ChangesPanel({
  active,
  record,
  status,
  nodes,
  error,
  onRemove,
}: ChangesPanelProps) {
  const entries: readonly ChangeSetEntry[] = record?.entries ?? [];
  const diff = active?.diff ?? emptyDiff();
  const groups = summarizeDiff(diff);
  const mutable = Boolean(
    record
    && canMutateChangeSet(record.status)
    && status !== 'evaluating'
    && status !== 'staging',
  );
  return (
    <section className="changes-panel" aria-label="ChangeSet review">
      <div className="panel-heading">
        <div>
          <span className="panel-kicker">Current proposal</span>
          <h2>Changes</h2>
        </div>
        <span className={`state-chip state-chip--${status}`}>{record?.status ?? status}</span>
      </div>
      <p className="changes-panel__title">{record?.title ?? 'No active ChangeSet'}</p>
      <p className="changes-panel__summary">{diffSummary(diff)}</p>
      {record && (
        <dl className="change-metadata">
          <div><dt>Base</dt><dd>revision {record.baseRevision}</dd></div>
          <div><dt>Version</dt><dd>{record.version}</dd></div>
          <div><dt>Entries</dt><dd>{record.entries.length}</dd></div>
        </dl>
      )}
      {status === 'stale' && (
        <p className="inline-warning">
          The project advanced after this proposal was created. Rebase before approval.
        </p>
      )}
      {error && <p className="form-error" role="alert">{error}</p>}
      <OperationList
        entries={entries}
        nodes={nodes}
        canRemove={mutable}
        onRemove={onRemove}
      />
      {groups.length > 0 && (
        <div className="diff-groups" aria-label="Candidate diff">
          {groups.map((group) => (
            <details className="diff-group" key={`${group.entity}:${group.change}`} open>
              <summary>
                <span>{group.entity}</span>
                <strong>{group.change}</strong>
                <small>{group.ids.length}</small>
              </summary>
              <ul>
                {group.ids.slice(0, 50).map((id) => <li key={id}><code>{id}</code></li>)}
              </ul>
              {group.ids.length > 50 && <p>+{group.ids.length - 50} more</p>}
            </details>
          ))}
        </div>
      )}
    </section>
  );
}

function diffSummary(diff: DocumentDiff): string {
  const groups = summarizeDiff(diff);
  if (groups.length === 0) return 'No candidate changes';
  const names: Record<string, string> = {
    Node: 'object',
    Material: 'material',
    Relation: 'relation',
    Constraint: 'constraint',
  };
  return groups.map((group) => {
    const noun = names[group.entity] ?? group.entity.toLowerCase();
    return `${group.ids.length} ${group.change} ${noun}${group.ids.length === 1 ? '' : 's'}`;
  }).join(' · ');
}

function emptyDiff(): DocumentDiff {
  return {
    addedNodeIds: [],
    updatedNodeIds: [],
    removedNodeIds: [],
    addedMaterialIds: [],
    updatedMaterialIds: [],
    removedMaterialIds: [],
    addedRelationIds: [],
    updatedRelationIds: [],
    removedRelationIds: [],
    addedConstraintIds: [],
    updatedConstraintIds: [],
    removedConstraintIds: [],
  };
}
