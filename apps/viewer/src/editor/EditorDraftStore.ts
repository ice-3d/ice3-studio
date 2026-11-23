import type { StudioNode } from '../api/StudioClient';
import { ExternalStore } from '../shared/ExternalStore';
import {
  createEditorValues,
  type EditorDraft,
  type EditorValues,
} from './types';

export type EditorDraftSnapshot = {
  drafts: Readonly<Record<string, EditorDraft>>;
};

export class EditorDraftStore extends ExternalStore<EditorDraftSnapshot> {
  private readonly previewOrigins = new Map<string, EditorValues>();

  constructor() {
    super({ drafts: {} });
  }

  ensure(node: StudioNode, revision: number): EditorValues {
    const current = this.snapshot.drafts[node.id];
    if (current) {
      if (current.baseRevision !== revision && current.status !== 'stale') {
        this.markStale(node.id, 'Project revision changed');
      }
      return structuredClone(this.snapshot.drafts[node.id]!.values);
    }
    const draft: EditorDraft = {
      nodeId: node.id,
      baseRevision: revision,
      baseNode: structuredClone(node),
      values: createEditorValues(node),
      stagedValues: null,
      history: [],
      future: [],
      status: 'clean',
      error: null,
    };
    this.setDraft(draft);
    return structuredClone(draft.values);
  }

  commit(nodeId: string, next: EditorValues): void {
    const draft = this.requireDraft(nodeId);
    this.setDraft({
      ...draft,
      history: [...draft.history, draft.values].slice(-100),
      future: [],
      values: structuredClone(next),
      status: 'dirty',
      error: null,
    });
  }

  beginPreview(nodeId: string): void {
    this.previewOrigins.set(nodeId, structuredClone(this.requireDraft(nodeId).values));
  }

  updatePreview(nodeId: string, next: EditorValues): void {
    const draft = this.requireDraft(nodeId);
    if (!this.previewOrigins.has(nodeId)) {
      this.previewOrigins.set(nodeId, structuredClone(draft.values));
    }
    this.setDraft({
      ...draft,
      values: structuredClone(next),
      status: 'dirty',
      error: null,
    });
  }

  finishPreview(nodeId: string): void {
    const origin = this.previewOrigins.get(nodeId);
    this.previewOrigins.delete(nodeId);
    const draft = this.requireDraft(nodeId);
    if (!origin || JSON.stringify(origin) === JSON.stringify(draft.values)) return;
    this.setDraft({
      ...draft,
      history: [...draft.history, origin].slice(-100),
      future: [],
    });
  }

  cancelPreview(nodeId: string): void {
    const origin = this.previewOrigins.get(nodeId);
    this.previewOrigins.delete(nodeId);
    if (!origin) return;
    const draft = this.requireDraft(nodeId);
    this.setDraft({ ...draft, values: origin, status: 'dirty' });
  }

  undo(nodeId: string): void {
    const draft = this.requireDraft(nodeId);
    const previous = draft.history.at(-1);
    if (!previous) return;
    this.setDraft({
      ...draft,
      values: structuredClone(previous),
      history: draft.history.slice(0, -1),
      future: [structuredClone(draft.values), ...draft.future].slice(0, 100),
      status: 'dirty',
      error: null,
    });
  }

  redo(nodeId: string): void {
    const draft = this.requireDraft(nodeId);
    const next = draft.future[0];
    if (!next) return;
    this.setDraft({
      ...draft,
      values: structuredClone(next),
      history: [...draft.history, structuredClone(draft.values)].slice(-100),
      future: draft.future.slice(1),
      status: 'dirty',
      error: null,
    });
  }

  markStaged(nodeId: string): void {
    const draft = this.requireDraft(nodeId);
    this.setDraft({
      ...draft,
      stagedValues: structuredClone(draft.values),
      status: 'staged',
      error: null,
    });
  }

  markStale(nodeId: string, message: string): void {
    const draft = this.requireDraft(nodeId);
    this.setDraft({ ...draft, status: 'stale', error: message });
  }

  markStageFailed(nodeId: string, message: string): void {
    const draft = this.requireDraft(nodeId);
    this.setDraft({ ...draft, status: 'failed', error: message });
  }

  restoreStagedAsDirty(): void {
    const drafts = Object.fromEntries(
      Object.entries(this.snapshot.drafts).map(([nodeId, draft]) => [
        nodeId,
        draft.stagedValues
          ? {
              ...draft,
              stagedValues: null,
              status: draft.status === 'staged' ? 'dirty' as const : draft.status,
            }
          : draft,
      ]),
    );
    this.update({ drafts });
  }

  hasDirtyDrafts(): boolean {
    return Object.values(this.snapshot.drafts).some((draft) => draft.status !== 'clean');
  }

  discard(nodeId: string): void {
    this.clear(nodeId);
  }

  clear(nodeId: string): void {
    const drafts = { ...this.snapshot.drafts };
    delete drafts[nodeId];
    this.previewOrigins.delete(nodeId);
    this.update({ drafts });
  }

  private requireDraft(nodeId: string): EditorDraft {
    const draft = this.snapshot.drafts[nodeId];
    if (!draft) throw new Error(`Editor draft does not exist: ${nodeId}`);
    return draft;
  }

  private setDraft(draft: EditorDraft): void {
    this.update({
      drafts: { ...this.snapshot.drafts, [draft.nodeId]: draft },
    });
  }
}
