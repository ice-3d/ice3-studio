export type StudioNode = {
  id: string;
  name: string;
  parentId: string | null;
  metadata: Record<string, string>;
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
