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
  if (typeof metadata.lineId === 'string' && metadata.lineId) {
    return metadata.lineId;
  }
  if (
    typeof metadata.supportLineId === 'string'
    && metadata.supportLineId
    && document
  ) {
    const connection = document.scene.nodes.find((candidate) => {
      const candidateMetadata = metadataOf(candidate);
      return candidate.content?.type === 'connection'
        && (
          candidateMetadata.supportLineId === metadata.supportLineId
          || candidateMetadata.lineId === metadata.supportLineId
        );
    });
    const lineId = connection && metadataOf(connection).lineId;
    return typeof lineId === 'string' && lineId ? lineId : null;
  }
  return null;
}
