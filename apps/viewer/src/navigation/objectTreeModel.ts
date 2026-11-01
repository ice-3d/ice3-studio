import type { StudioNode } from '../api/StudioClient';

export type TreeNode = {
  id: string;
  name: string;
  children: TreeNode[];
};

export function buildObjectTree(nodes: readonly StudioNode[]): TreeNode[] {
  const byId = new Map<string, TreeNode>();
  for (const node of nodes) {
    byId.set(node.id, { id: node.id, name: node.name, children: [] });
  }
  const roots: TreeNode[] = [];
  for (const node of nodes) {
    const treeNode = byId.get(node.id)!;
    const parent = node.parentId ? byId.get(node.parentId) : undefined;
    if (parent) parent.children.push(treeNode);
    else roots.push(treeNode);
  }
  return roots;
}

export function filterObjectTree(nodes: readonly TreeNode[], query: string): TreeNode[] {
  const normalized = query.trim().toLocaleLowerCase();
  if (!normalized) return [...nodes];

  return nodes.flatMap((node) => {
    if (node.name.toLocaleLowerCase().includes(normalized)) {
      return [{ ...node, children: [...node.children] }];
    }
    const children = filterObjectTree(node.children, normalized);
    return children.length > 0 ? [{ ...node, children }] : [];
  });
}

export function countObjectTree(nodes: readonly TreeNode[]): number {
  return nodes.reduce(
    (count, node) => count + 1 + countObjectTree(node.children),
    0,
  );
}
