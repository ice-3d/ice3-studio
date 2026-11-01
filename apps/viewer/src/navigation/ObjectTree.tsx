import { useState, type CSSProperties } from 'react';
import type { TreeNode } from './objectTreeModel';

type Props = {
  nodes: TreeNode[];
  selectedIds: string[];
  onSelect: (id: string) => void;
  query?: string;
};

export function ObjectTree({ nodes, selectedIds, onSelect, query = '' }: Props) {
  const [collapsedIds, setCollapsedIds] = useState<ReadonlySet<string>>(
    () => new Set(),
  );

  return (
    <ul role="tree" aria-label="Project objects" className="object-tree">
      {nodes.map((node) => (
        <ObjectTreeItem
          key={node.id}
          node={node}
          depth={0}
          selectedIds={selectedIds}
          onSelect={onSelect}
          query={query}
          collapsedIds={collapsedIds}
          onToggle={(id) => setCollapsedIds((current) => {
            const next = new Set(current);
            if (next.has(id)) next.delete(id);
            else next.add(id);
            return next;
          })}
        />
      ))}
    </ul>
  );
}

type ItemProps = {
  node: TreeNode;
  depth: number;
  selectedIds: string[];
  onSelect: (id: string) => void;
  query: string;
  collapsedIds: ReadonlySet<string>;
  onToggle: (id: string) => void;
};

function ObjectTreeItem({
  node,
  depth,
  selectedIds,
  onSelect,
  query,
  collapsedIds,
  onToggle,
}: ItemProps) {
  const hasChildren = node.children.length > 0;
  const expanded = hasChildren && (query.trim().length > 0 || !collapsedIds.has(node.id));
  const selected = selectedIds.includes(node.id);
  return (
    <li
      role="treeitem"
      aria-selected={selected}
      aria-expanded={hasChildren ? expanded : undefined}
      className="object-tree__item"
    >
      <div
        className={`object-tree__row${selected ? ' object-tree__row--selected' : ''}`}
        style={{ '--tree-depth': depth } as CSSProperties}
      >
        {hasChildren ? (
          <button
            type="button"
            className="object-tree__disclosure"
            aria-label={`${expanded ? 'Collapse' : 'Expand'} ${node.name}`}
            onClick={() => onToggle(node.id)}
          >
            <svg viewBox="0 0 12 12" aria-hidden="true">
              <path d="m4 2.5 3.5 3.5L4 9.5" />
            </svg>
          </button>
        ) : (
          <span className="object-tree__spacer" aria-hidden="true" />
        )}
        <button
          type="button"
          className="object-tree__label"
          aria-current={selected ? 'true' : undefined}
          onClick={() => onSelect(node.id)}
        >
          <span className={`object-tree__kind${hasChildren ? ' object-tree__kind--group' : ''}`} aria-hidden="true" />
          <span>{node.name}</span>
        </button>
      </div>
      {expanded && (
        <ul role="group" className="object-tree__children">
          {node.children.map((child) => (
            <ObjectTreeItem
              key={child.id}
              node={child}
              depth={depth + 1}
              selectedIds={selectedIds}
              onSelect={onSelect}
              query={query}
              collapsedIds={collapsedIds}
              onToggle={onToggle}
            />
          ))}
        </ul>
      )}
    </li>
  );
}
