import type { ChangeSetEntry, StudioNode } from '../api/StudioClient';
import { operationFieldSummary, operationTarget } from './reviewModel';

type OperationListProps = {
  entries: readonly ChangeSetEntry[];
  nodes: readonly StudioNode[];
  canRemove?: boolean;
  onRemove?: (entryId: string) => void;
};

export function OperationList({
  entries,
  nodes,
  canRemove = false,
  onRemove,
}: OperationListProps) {
  if (entries.length === 0) {
    return (
      <div className="empty-state">
        <strong>No staged operations</strong>
        <span>Use Inspect to add a typed operation.</span>
      </div>
    );
  }
  const names = new Map(nodes.map((node) => [node.id, node.name]));
  return (
    <ol className="operation-list" aria-label="ChangeSet operations">
      {entries.map((entry) => {
        const target = operationTarget(entry.operation);
        const targetName = target ? names.get(target) : null;
        const source = entry.source.label ?? (entry.source.kind === 'agent' ? 'Agent' : 'Human');
        const fields = operationFieldSummary(entry.operation);
        return (
          <li className="operation-card" key={entry.id}>
            <div className="operation-card__heading">
              <code>{entry.operation.type}</code>
              <span>{source}</span>
            </div>
            <strong>{targetName ?? target ?? 'ChangeSet'}</strong>
            {fields.length > 0 && (
              <dl className="operation-card__fields">
                {fields.map((field) => (
                  <div key={field.label}>
                    <dt>{field.label}</dt>
                    <dd>{field.value}</dd>
                  </div>
                ))}
              </dl>
            )}
            {targetName && target && <small>{target}</small>}
            {canRemove && onRemove && (
              <button
                type="button"
                className="button button--quiet operation-card__remove"
                aria-label={`Remove ${entry.operation.type} for ${targetName ?? target ?? entry.id}`}
                onClick={() => onRemove(entry.id)}
              >
                Remove
              </button>
            )}
          </li>
        );
      })}
    </ol>
  );
}
