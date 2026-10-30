import type { TreeNode } from './objectTreeModel';

type Props = {
  nodes: TreeNode[];
  selectedIds: string[];
  onSelect: (id: string) => void;
};

export function ObjectTree({ nodes, selectedIds, onSelect }: Props) {
  return (
    <ul role="tree" aria-label="Project objects">
      {nodes.map((node) => (
        <li role="treeitem" key={node.id} aria-selected={selectedIds.includes(node.id)}>
          <button type="button" onClick={() => onSelect(node.id)}>{node.name}</button>
          {node.children.length > 0 && (
            <ObjectTree nodes={node.children} selectedIds={selectedIds} onSelect={onSelect} />
          )}
        </li>
      ))}
    </ul>
  );
}
