import type { StudioDocument, StudioNode } from '../api/StudioClient';

function metadataOf(node: StudioNode): Record<string, string> {
  const contentMetadata = node.content?.metadata;
  return {
    ...(typeof contentMetadata === 'object' && contentMetadata ? contentMetadata : {}),
    ...node.metadata,
  } as Record<string, string>;
}

export function routeLineIdForNode(
  node: StudioNode | undefined,
  document: StudioDocument | null,
): string | null {
  if (!node) return null;
  const metadata = metadataOf(node);
  const supportLineId = typeof metadata.supportLineId === 'string'
    ? metadata.supportLineId
    : null;
  if (metadata.role === 'pipe-support' && supportLineId && document) {
    const lineId = connectionLineIdForSupport(supportLineId, document);
    if (lineId) return lineId;
  }
  if (typeof metadata.lineId === 'string' && metadata.lineId) return metadata.lineId;
  if (
    supportLineId
    && document
  ) {
    return connectionLineIdForSupport(supportLineId, document);
  }
  return null;
}

function connectionLineIdForSupport(
  supportLineId: string,
  document: StudioDocument,
): string | null {
  const connection = document.scene.nodes.find((candidate) => {
    const candidateMetadata = metadataOf(candidate);
    return candidate.content?.type === 'connection'
      && (
        candidateMetadata.supportLineId === supportLineId
        || candidateMetadata.lineId === supportLineId
      );
  });
  const lineId = connection && metadataOf(connection).lineId;
  return typeof lineId === 'string' && lineId ? lineId : null;
}
