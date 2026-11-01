import { useDeferredValue, useState } from 'react';
import { countObjectTree, filterObjectTree, type TreeNode } from './objectTreeModel';
import { ObjectTree } from './ObjectTree';

type Props = {
  nodes: TreeNode[];
  selectedIds: string[];
  onSelect: (id: string) => void;
};

export function ObjectBrowser({ nodes, selectedIds, onSelect }: Props) {
  const [query, setQuery] = useState('');
  const deferredQuery = useDeferredValue(query);
  const filteredNodes = filterObjectTree(nodes, deferredQuery);
  const total = countObjectTree(nodes);
  const visible = countObjectTree(filteredNodes);

  return (
    <div className="object-browser">
      <header className="object-browser__header">
        <div>
          <span className="panel-kicker">Project model</span>
          <h2>Objects</h2>
        </div>
        <span className="object-browser__count" aria-label={`${visible} objects`}>
          {deferredQuery.trim() ? `${visible}/${total}` : total}
        </span>
      </header>
      <label className="object-search">
        <svg viewBox="0 0 20 20" aria-hidden="true">
          <circle cx="8.5" cy="8.5" r="5.5" />
          <path d="m12.7 12.7 4.1 4.1" />
        </svg>
        <input
          type="search"
          value={query}
          placeholder="Search objects"
          aria-label="Search objects"
          onChange={(event) => setQuery(event.target.value)}
        />
        {query ? (
          <button
            type="button"
            className="object-search__clear"
            aria-label="Clear object search"
            onClick={() => setQuery('')}
          >
            ×
          </button>
        ) : null}
      </label>
      <div className="object-browser__content">
        {filteredNodes.length > 0 ? (
          <ObjectTree
            nodes={filteredNodes}
            selectedIds={selectedIds}
            onSelect={onSelect}
            query={deferredQuery}
          />
        ) : (
          <div className="empty-state empty-state--compact">
            <strong>No matching objects</strong>
            <span>Try a tag, equipment name, or stage.</span>
          </div>
        )}
      </div>
    </div>
  );
}
