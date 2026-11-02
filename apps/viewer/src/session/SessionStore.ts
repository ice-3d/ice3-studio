import type {
  ComputeStatus,
  Diagnostic,
  StudioCapabilities,
  StudioDocument,
  StudioReferenceAsset,
  StudioReferenceBindingEntry,
  StudioReferenceBindingPage,
  StudioTransform,
} from '../api/StudioClient';
import { ExternalStore } from '../shared/ExternalStore';

type DocumentSource = {
  document(): Promise<StudioDocument>;
  validation(): Promise<Diagnostic[]>;
  preview?(revision: number): Promise<{ url: string }>;
  capabilities?(): Promise<StudioCapabilities>;
  referenceAsset?(assetId: string): Promise<StudioReferenceAsset>;
  referenceBindings?(
    assetId: string,
    cursor: string | null,
    limit: number,
  ): Promise<StudioReferenceBindingPage>;
};

export type StudioReferenceBinding = StudioReferenceBindingEntry & {
  nodeId: string | null;
  transform: StudioTransform;
  visible: boolean;
  materialId: string | null;
  materialColor: string | null;
};

export type StudioReferenceSession = {
  asset: StudioReferenceAsset;
  bindings: StudioReferenceBinding[];
};

export type SessionSnapshot = {
  projectId: string;
  revision: number;
  document: StudioDocument | null;
  diagnostics: Diagnostic[];
  selectionIds: string[];
  connection: 'connecting' | 'connected' | 'error';
  previewUrl: string | null;
  compute: ComputeStatus | null;
  reference: StudioReferenceSession | null;
};

export class SessionStore extends ExternalStore<SessionSnapshot> {
  private generation = 0;

  constructor(private readonly source: DocumentSource) {
    super({
      projectId: '',
      revision: 0,
      document: null,
      diagnostics: [],
      selectionIds: [],
      connection: 'connecting',
      previewUrl: null,
      compute: null,
      reference: null,
    });
  }

  async refresh(): Promise<void> {
    const generation = ++this.generation;
    try {
      const [document, diagnostics, capabilities] = await Promise.all([
        this.source.document(),
        this.source.validation(),
        this.source.capabilities?.() ?? Promise.resolve(null),
      ]);
      const reference = await loadReference(this.source, document);
      if (generation !== this.generation) return;
      this.update({
        ...this.snapshot,
        projectId: document.projectId,
        revision: document.revision,
        document,
        diagnostics,
        connection: 'connected',
        previewUrl: this.snapshot.previewUrl,
        compute: capabilities?.compute ?? this.snapshot.compute,
        reference,
      });
      if (!reference && this.source.preview) {
        try {
          const preview = await this.source.preview(document.revision);
          if (generation === this.generation) {
            this.update({ ...this.snapshot, previewUrl: preview.url });
          }
        } catch {
          // The last valid preview remains mounted while document review continues.
        }
      }
    } catch {
      if (generation === this.generation) {
        this.update({ ...this.snapshot, connection: 'error' });
      }
    }
  }

  select(selectionIds: readonly string[]): void {
    this.update({ ...this.snapshot, selectionIds: [...selectionIds] });
  }
}

async function loadReference(
  source: DocumentSource,
  document: StudioDocument,
): Promise<StudioReferenceSession | null> {
  const referenceNode = document.scene.nodes.find((node) => (
    node.content?.type === 'reference_instance'
    && typeof node.content.assetId === 'string'
  ));
  const assetId = referenceNode?.content?.assetId;
  if (typeof assetId !== 'string' || !source.referenceAsset) return null;
  const asset = await source.referenceAsset(assetId);
  const nodesBySelection = new Map(
    document.scene.nodes
      .filter((node) => node.content?.type === 'reference_instance')
      .map((node) => [String(node.content?.selectionId ?? ''), node]),
  );
  const entries: StudioReferenceBindingEntry[] = [];
  if (source.referenceBindings) {
    let cursor: string | null = null;
    for (let pageIndex = 0; pageIndex < 1_000; pageIndex += 1) {
      const page = await source.referenceBindings(assetId, cursor, 100);
      entries.push(...page.items);
      if (!page.nextCursor || page.nextCursor === cursor) break;
      cursor = page.nextCursor;
    }
  } else {
    entries.push(...document.scene.nodes
      .filter((node) => node.content?.type === 'reference_instance')
      .map((node) => ({
        selectionId: String(node.content?.selectionId ?? ''),
        name: node.name,
      })));
  }
  // 工程指派了材质就用工程的颜色；否则回退到资产为该绑定声明的槽位颜色。
  const slotColors = new Map(
    (asset.descriptor.materialSlots ?? [])
      .filter((slot) => Boolean(slot.color))
      .map((slot) => [slot.id, String(slot.color)]),
  );
  return {
    asset,
    bindings: entries.map((entry) => {
      const node = nodesBySelection.get(entry.selectionId);
      return {
        ...entry,
        nodeId: node?.id ?? null,
        transform: node?.transform ?? identityTransform(),
        visible: node?.visible ?? true,
        materialId: node?.materialId ?? null,
        materialColor: materialColor(document, node?.materialId)
          ?? (entry.slot ? slotColors.get(entry.slot) ?? null : null),
      };
    }),
  };
}

function materialColor(
  document: StudioDocument,
  materialId: string | null | undefined,
): string | null {
  if (!materialId) return null;
  const material = document.materials?.find((candidate) => candidate.id === materialId);
  if (!material) return null;
  return `#${material.baseColor.slice(0, 3).map((channel) => (
    Math.round(Math.min(1, Math.max(0, channel)) * 255)
      .toString(16)
      .padStart(2, '0')
  )).join('')}`;
}

function identityTransform(): StudioTransform {
  return {
    translation: { x: 0, y: 0, z: 0 },
    rotation: [0, 0, 0, 1],
    scale: { x: 1, y: 1, z: 1 },
  };
}
