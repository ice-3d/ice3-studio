import type {
  Diagnostic,
  StudioCapabilities,
  StudioDocument,
  ToolchainStatus,
} from '../api/StudioClient';
import { ExternalStore } from '../shared/ExternalStore';

type DocumentSource = {
  document(): Promise<StudioDocument>;
  validation(): Promise<Diagnostic[]>;
  preview?(revision: number): Promise<{ url: string }>;
  capabilities?(): Promise<StudioCapabilities>;
};

export type SessionSnapshot = {
  projectId: string;
  revision: number;
  document: StudioDocument | null;
  diagnostics: Diagnostic[];
  selectionIds: string[];
  connection: 'connecting' | 'connected' | 'error';
  previewUrl: string | null;
  toolchain: ToolchainStatus | null;
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
      toolchain: null,
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
      if (generation !== this.generation) return;
      this.update({
        ...this.snapshot,
        projectId: document.projectId,
        revision: document.revision,
        document,
        diagnostics,
        connection: 'connected',
        previewUrl: this.snapshot.previewUrl,
        toolchain: capabilities?.toolchain ?? this.snapshot.toolchain,
      });
      if (this.source.preview) {
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
