export type StudioNode = {
  id: string;
  name: string;
  parentId: string | null;
  visible?: boolean;
  materialId?: string | null;
  metadata: Record<string, string>;
  transform?: StudioTransform;
  content?: Record<string, unknown>;
};

export type StudioVec3 = {
  x: number;
  y: number;
  z: number;
};

export type StudioTransform = {
  translation: StudioVec3;
  rotation: [number, number, number, number];
  scale: StudioVec3;
};

export type StudioBounds = {
  min: StudioVec3;
  max: StudioVec3;
};

export type StudioReferenceAsset = {
  descriptor: {
    assetId: string;
    sourceDigest: string;
    preparedDigest: string | null;
    tilesetDigest: string | null;
    bounds: StudioBounds;
    stats: {
      sourceNodes: number;
      effectiveTriangles: number;
      uniqueMeshes: number;
      materials: number;
      bindings: number;
    };
    materialSlots: Array<{ id: string; color: string | null; role: string | null }>;
    displayExposure?: number;
    importedAt: string;
  };
  tilesetUrl: string | null;
  preparedPreviewUrl: string | null;
  bindingManifestUrl: string;
};

export type StudioReferenceBindingEntry = {
  selectionId: string;
  name: string;
  kind?: string | null;
  metadata?: Record<string, string>;
  bounds?: StudioBounds;
  transform?: StudioTransform;
  materialIds?: string[];
  slot?: string | null;
  hidden?: boolean;
};

export type StudioReferenceBindingPage = {
  items: StudioReferenceBindingEntry[];
  nextCursor: string | null;
};

export type ParameterValue =
  | number
  | StudioVec3
  | [number, number, number, number]
  | string;

export type ComponentDefinition = {
  id: string;
  name: string;
  parameters: Array<{
    name: string;
    valueType: Record<string, unknown> & { kind: string };
    default: ParameterValue;
    unit?: string | null;
  }>;
};

export type ComponentInstance = {
  definitionId: string;
  parameters: Record<string, ParameterValue>;
  overrides: Record<string, unknown>;
  metadata: Record<string, string>;
};

export type StudioDocument = {
  projectId: string;
  revision: number;
  scene: { nodes: StudioNode[] };
  components: ComponentDefinition[];
  materials?: StudioMaterial[];
};

export type StudioMaterial = {
  id: string;
  name: string;
  baseColor: [number, number, number, number];
};

export type Diagnostic = {
  code: string;
  pointer: string;
  message: string;
};

export type DocumentDiff = {
  addedNodeIds: string[];
  updatedNodeIds: string[];
  removedNodeIds: string[];
  addedMaterialIds: string[];
  updatedMaterialIds: string[];
  removedMaterialIds: string[];
  addedRelationIds: string[];
  updatedRelationIds: string[];
  removedRelationIds: string[];
  addedConstraintIds: string[];
  updatedConstraintIds: string[];
  removedConstraintIds: string[];
};

export type ComputeStatus = {
  mode: 'auto' | 'cpu' | 'gpu';
  incrementalSceneCache?: boolean;
  selectedBackend: 'cpu' | 'gpu';
  adapter: string;
  tier: string;
  memoryBudgetBytes: number;
  health: 'probing' | 'ready' | 'unavailable' | string;
  fallbackReason: string | null;
  queueDepth: number;
  maxConcurrentJobs: number;
  gpuThresholdTriangles: number;
  lastKernelMs: number;
  lastUploadBytes: number;
  lastReadbackBytes: number;
  lastCacheHits: number;
  lastEvaluationMode?: string;
  residentBytes?: number;
  peakResidentBytes?: number;
  topologyHits?: number;
  residentHits?: number;
  evictions?: number;
  previewPacketBytes?: number;
  browserUploadBytes?: number;
  snapshotReadbackBytes?: number;
  residentGeometry?: {
    enabled: boolean;
    geometryCount: number;
    residentBytes: number;
    peakResidentBytes: number;
    topologyHits: number;
    residentHits: number;
    evictions: number;
  };
  residentHotPathHashes?: number;
  handleLookupMs?: number;
  lastGeometryPhases?: {
    lookupMs: number;
    planningMs: number;
    validationMs: number;
    encodeMs: number;
    submitMs: number;
    gpuKernelMs: number | null;
    waitMs: number;
    snapshotMs: number;
    transportMs: number;
    browserUploadMs: number;
  };
  sceneSession?: {
    enabled: boolean;
    sessionId: string | null;
    state: 'idle' | 'pending' | 'failed' | 'lost';
    publishedRevision: number;
    targetRevision: number;
    pendingPatches: number;
    pendingSubmissions: number;
    geometryCount: number;
    topologyCount: number;
    attributeCount: number;
    instanceCount: number;
    residentBytes: number;
    peakResidentBytes: number;
    dirtyRangeBytes: number;
    readbackBytes: number;
    submissionCount?: number;
    geometryDispatchCount?: number;
    fallbackReason: string | null;
    phases: Record<string, number>;
  };
};

export type StudioCapabilities = {
  apiVersion: string;
  schemaVersion: number;
  methods: string[];
  compute: ComputeStatus;
};

export type StudioPreview = {
  revision: number;
  url: string;
  packetUrl: string | null;
  glbUrl: string;
};

export type ImpactCounts = { added: number; updated: number; removed: number };

export type Vec3Like = { x: number; y: number; z: number };

export type SpatialPreflight = {
  changedBounds: { min: Vec3Like; max: Vec3Like } | null;
  sceneBounds: { min: Vec3Like; max: Vec3Like } | null;
  outsideNodeCount: number;
  changedVisibleNodes: number;
};

export type ChangeSetImpact = {
  changeSetId: string;
  version: number;
  baseRevision: number;
  status: string;
  candidateDigest: string | null;
  diagnosticCount: number;
  impact: {
    scope: string;
    fullSceneRebuildRequired: boolean;
    sourceNodes: ImpactCounts;
    derivedNodes: ImpactCounts;
    relations: ImpactCounts;
    components: ImpactCounts;
    materials: ImpactCounts;
  };
};

export type ChangeSetEvaluation = {
  changeSetId: string;
  version: number;
  baseRevision: number;
  status: string;
  candidateDigest: string | null;
  diff: DocumentDiff;
  diagnostics: Diagnostic[];
  spatial?: SpatialPreflight | null;
  previewUrl?: string;
};

type Envelope<T> = {
  result?: T;
  error?: {
    code: number;
    message: string;
    diagnosticCode?: string;
    data?: unknown;
  };
};

export class StudioApiError extends Error {
  constructor(
    message: string,
    readonly code?: number,
    readonly diagnosticCode?: string,
    readonly data?: unknown,
  ) {
    super(message);
  }
}

export class StudioClient {
  private requestId = 0;
  private readonly token =
    new URLSearchParams(window.location.search).get('token')
    ?? localStorage.getItem('ice3-studio-token')
    ?? __STUDIO_TOKEN__;

  health(): Promise<{ apiVersion: string; schemaVersion: number }> {
    return fetch('/health', { credentials: 'include' }).then((response) => response.json());
  }

  capabilities(): Promise<StudioCapabilities> {
    return this.rpc('system.capabilities', {});
  }

  document(): Promise<StudioDocument> {
    return this.rpc('document.get', {});
  }

  validation(): Promise<Diagnostic[]> {
    return this.rpc('validation.run', {});
  }

  referenceAsset(assetId: string): Promise<StudioReferenceAsset> {
    return this.rpc('referenceAsset.get', { assetId });
  }

  referenceBindings(
    assetId: string,
    cursor: string | null,
    limit: number,
  ): Promise<StudioReferenceBindingPage> {
    return this.rpc('referenceAsset.bindings', { assetId, cursor, limit });
  }

  async preview(revision: number): Promise<StudioPreview> {
    const value = await this.rpc<{ packetUrl?: string | null }>(
      'export.runtimePreview',
      { revision },
    );
    const glbUrl = `/previews/${revision}`;
    const packetUrl = value.packetUrl ?? null;
    return {
      revision,
      url: packetUrl ?? glbUrl,
      packetUrl,
      glbUrl,
    };
  }

  async candidatePreview(
    candidateDigest: string,
  ): Promise<{ url: string; packetUrl: string | null }> {
    const preview = await this.rpc<{ url: string; packetUrl?: string | null }>(
      'export.runtimePreview',
      { candidateDigest },
    );
    const packetUrl = preview.packetUrl ?? null;
    return {
      url: packetUrl ?? preview.url,
      packetUrl,
    };
  }

  createChangeSet(baseRevision: number, title: string): Promise<{ id: string; version: number }> {
    return this.rpc('changeset.create', {
      baseRevision,
      title,
      idempotencyKey: `viewer-create-${baseRevision}-${title}`,
    });
  }

  addChangeSetOperations(
    changeSetId: string,
    expectedVersion: number,
    operations: Record<string, unknown>[],
    source: { kind: 'human' | 'agent'; label?: string },
  ): Promise<{ version: number }> {
    return this.rpc('changeset.addOperations', {
      changeSetId,
      expectedVersion,
      idempotencyKey: `viewer-add-${changeSetId}-${expectedVersion}`,
      entries: operations.map((operation) => ({ source, operation })),
    });
  }

  removeChangeSetOperations(
    changeSetId: string,
    expectedVersion: number,
    entryIds: string[],
  ): Promise<{ version: number }> {
    return this.rpc('changeset.removeOperations', {
      changeSetId,
      expectedVersion,
      entryIds,
      idempotencyKey: `viewer-remove-${changeSetId}-${expectedVersion}`,
    });
  }

  changeSetImpact(changeSetId: string, expectedVersion: number): Promise<ChangeSetImpact> {
    return this.rpc('changeset.impact', { changeSetId, expectedVersion });
  }

  evaluateChangeSet(changeSetId: string, expectedVersion: number): Promise<ChangeSetEvaluation> {
    return this.rpc('changeset.evaluate', {
      changeSetId,
      expectedVersion,
      includeCandidate: true,
    });
  }

  commitChangeSet(
    changeSetId: string,
    expectedVersion: number,
    approvedDigest: string,
    idempotencyKey: string,
  ): Promise<{ revision: number }> {
    return this.rpc('changeset.commit', {
      changeSetId,
      expectedVersion,
      approvedDigest,
      idempotencyKey,
    });
  }

  subscribe(onEvent: (event: { event: string; revision?: number }) => void): () => void {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const socket = new WebSocket(
      `${protocol}//${window.location.host}/ws?token=${encodeURIComponent(this.token)}`,
    );
    socket.addEventListener('message', (message) => {
      const event = JSON.parse(String(message.data)) as { event?: string; revision?: number };
      if (event.event) onEvent(event as { event: string; revision?: number });
    });
    return () => socket.close();
  }

  private async rpc<T>(method: string, params: Record<string, unknown>): Promise<T> {
    this.requestId += 1;
    const response = await fetch('/rpc', {
      method: 'POST',
      credentials: 'include',
      headers: {
        'content-type': 'application/json',
        ...(this.token ? { authorization: `Bearer ${this.token}` } : {}),
      },
      body: JSON.stringify({ jsonrpc: '2.0', id: this.requestId, method, params }),
    });
    const envelope = await response.json() as Envelope<T>;
    if (envelope.error) {
      throw new StudioApiError(
        envelope.error.message,
        envelope.error.code,
        envelope.error.diagnosticCode,
        envelope.error.data,
      );
    }
    return envelope.result as T;
  }
}
