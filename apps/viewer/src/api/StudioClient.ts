export type StudioNode = {
  id: string;
  name: string;
  parentId: string | null;
  visible?: boolean;
  metadata: Record<string, string>;
  transform?: Record<string, unknown>;
  content?: Record<string, unknown>;
};

export type StudioDocument = {
  projectId: string;
  revision: number;
  scene: { nodes: StudioNode[] };
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

export type ToolchainStatus = {
  state: 'starting' | 'ready' | 'busy' | 'unavailable' | 'incompatible' | 'failed';
  contractVersion: string | null;
  packageVersion: string | null;
  capabilities: string[];
  reason: string | null;
};

export type StudioCapabilities = {
  apiVersion: string;
  schemaVersion: number;
  methods: string[];
  toolchain: ToolchainStatus;
};

export type ChangeSetEvaluation = {
  changeSetId: string;
  version: number;
  baseRevision: number;
  status: string;
  candidateDigest: string | null;
  diff: DocumentDiff;
  diagnostics: Diagnostic[];
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

  preview(revision: number): Promise<{ revision: number; url: string }> {
    return this.rpc('export.runtimePreview', { revision }).then((value) => ({
      revision,
      url: `/previews/${revision}`,
    }));
  }

  candidatePreview(candidateDigest: string): Promise<{ url: string }> {
    return this.rpc('export.runtimePreview', { candidateDigest });
  }

  createChangeSet(baseRevision: number, title: string): Promise<{ id: string; version: number }> {
    return this.rpc('changeset.create', { baseRevision, title });
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
      entries: operations.map((operation) => ({ source, operation })),
    });
  }

  removeChangeSetOperations(
    changeSetId: string,
    expectedVersion: number,
    entryIds: string[],
  ): Promise<{ version: number }> {
    return this.rpc('changeset.removeOperations', { changeSetId, expectedVersion, entryIds });
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
