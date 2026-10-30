export type NodeSnapshot = {
  id: string;
  name: string;
  parentId: string | null;
  metadata: Record<string, string>;
};

export type SceneSnapshot = {
  revision: number;
  projectId: string;
  scene: { nodes: NodeSnapshot[] };
};

export type ValidationReport = Array<{
  code: string;
  pointer: string;
  message: string;
}>;

type RpcEnvelope<T> = {
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

  async health(): Promise<{ apiVersion: string; schemaVersion: number }> {
    const response = await fetch('/health');
    if (!response.ok) throw new StudioApiError(`Health check failed: ${response.status}`);
    return response.json();
  }

  async scene(): Promise<SceneSnapshot> {
    return this.rpc<SceneSnapshot>('document.get', {});
  }

  async validation(): Promise<ValidationReport> {
    return this.rpc<ValidationReport>('validation.run', {});
  }

  async preview(revision: number): Promise<{ url: string }> {
    const preview = await this.rpc<{ revision: number }>(
      'export.runtimePreview',
      { revision },
    );
    return { url: `/previews/${preview.revision}` };
  }

  subscribe(onRevision: (revision: number) => void): () => void {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const socket = new WebSocket(
      `${protocol}//${window.location.host}/ws?token=${encodeURIComponent(this.token)}`,
    );
    socket.addEventListener('message', (event) => {
      const message = JSON.parse(String(event.data)) as {
        event?: string;
        revision?: number;
      };
      if (message.event && typeof message.revision === 'number') {
        onRevision(message.revision);
      }
    });
    return () => socket.close();
  }

  private async rpc<T>(
    method: string,
    params: Record<string, unknown>,
  ): Promise<T> {
    this.requestId += 1;
    const response = await fetch('/rpc', {
      method: 'POST',
      headers: {
        'content-type': 'application/json',
        ...(this.token ? { authorization: `Bearer ${this.token}` } : {}),
      },
      body: JSON.stringify({
        jsonrpc: '2.0',
        id: this.requestId,
        method,
        params,
      }),
    });
    const envelope = await response.json() as RpcEnvelope<T>;
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
