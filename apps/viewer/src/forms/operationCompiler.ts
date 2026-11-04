import type { StudioNode } from '../api/StudioClient';

export type Operation = Record<string, unknown> & { type: string };

export function compileNodeChanges(
  node: StudioNode,
  draft: {
    name?: string;
    visible?: boolean;
    metadata?: Record<string, string>;
  },
): Operation[] {
  const name = draft.name !== undefined && draft.name !== node.name
    ? draft.name
    : null;
  const visible = draft.visible !== undefined && draft.visible !== (node.visible ?? true)
    ? draft.visible
    : null;
  const metadata = draft.metadata
    && JSON.stringify(draft.metadata) !== JSON.stringify(node.metadata)
    ? draft.metadata
    : null;
  if (name === null && visible === null && metadata === null) return [];
  return [{
    type: 'node.update',
    nodeId: node.id,
    name,
    transform: null,
    visible,
    metadata,
  }];
}
