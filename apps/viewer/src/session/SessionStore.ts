import type { Diagnostic, StudioDocument } from '../api/StudioClient';
import { ExternalStore } from '../shared/ExternalStore';

type DocumentSource = {
  document(): Promise<StudioDocument>;
  validation(): Promise<Diagnostic[]>;
};

export type SessionSnapshot = {
  projectId: string;
  revision: number;
  document: StudioDocument | null;
  diagnostics: Diagnostic[];
  selectionIds: string[];
  connection: 'connecting' | 'connected' | 'error';
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
    });
  }

  async refresh(): Promise<void> {
    const generation = ++this.generation;
    try {
      const [document, diagnostics] = await Promise.all([
        this.source.document(),
        this.source.validation(),
      ]);
      if (generation !== this.generation) return;
      this.update({
        ...this.snapshot,
        projectId: document.projectId,
        revision: document.revision,
        document,
        diagnostics,
        connection: 'connected',
      });
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
