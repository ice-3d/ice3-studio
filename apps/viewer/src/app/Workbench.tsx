import './studioTokens.css';
import { WorkbenchHeader } from './WorkbenchHeader';
import { useSession, useSessionStore } from '../session/SessionProvider';
import { buildObjectTree } from '../navigation/objectTreeModel';
import { ObjectBrowser } from '../navigation/ObjectBrowser';
import { Viewport } from '../viewport/Viewport';
import { FormRenderer } from '../forms/FormRenderer';
import { useChangeSet, useChangeSetStore } from '../changeset/ChangeSetProvider';
import { AgentPanel } from '../agent/AgentPanel';
import { useEffect, useRef, useState } from 'react';
import { ComputePanel } from '../compute/ComputePanel';
import { ChangesPanel } from '../changeset/ChangesPanel';
import { EvidenceDrawer, type EvidenceTab } from './EvidenceDrawer';
import { mergeDiagnostics } from '../changeset/reviewModel';
import { RouteInspector } from '../route/RouteInspector';
import type { StudioTransform } from '../api/StudioClient';
import { useAgent, useAgentStore } from '../agent/AgentProvider';
import {
  useEditorDrafts,
  useEditorDraftStore,
} from '../editor/EditorDraftProvider';
import { compileDraftOperations } from '../editor/draftOperations';
import { TransformEditor } from '../editor/TransformEditor';
import type {
  EditorValues,
  GizmoMode,
  TransformSpace,
} from '../editor/types';
import type { SceneAdapter } from '../viewport/SceneAdapter';

export function Workbench() {
  const session = useSession();
  const sessionStore = useSessionStore();
  const agent = useAgent();
  const agentStore = useAgentStore();
  const changeSetStore = useChangeSetStore();
  const changeSet = useChangeSet();
  const editorDrafts = useEditorDrafts();
  const editorDraftStore = useEditorDraftStore();
  const [dock, setDock] = useState<'agent' | 'inspect' | 'changes'>('agent');
  const [gizmoMode, setGizmoMode] = useState<GizmoMode>('move');
  const [transformSpace, setTransformSpace] = useState<TransformSpace>('local');
  const [evidenceTab, setEvidenceTab] = useState<EvidenceTab>('operations');
  const [evidenceOpen, setEvidenceOpen] = useState(false);
  const viewportAdapter = useRef<SceneAdapter | null>(null);
  const nodes = session.document?.scene.nodes ?? [];
  const selected = nodes.find((node) => node.id === session.selectionIds[0]);
  const definitionId = typeof selected?.content?.definitionId === 'string'
    ? selected.content.definitionId
    : null;
  const selectedDefinition = definitionId
    ? session.document?.components.find((definition) => definition.id === definitionId)
    : undefined;
  const selectedDraft = selected ? editorDrafts.drafts[selected.id] : undefined;
  const editorValues = selectedDraft?.values;

  useEffect(() => {
    if (dock === 'inspect' && selected && !editorDrafts.drafts[selected.id]) {
      editorDraftStore.ensure(selected, session.revision);
    }
  }, [dock, editorDraftStore, editorDrafts.drafts, selected, session.revision]);

  useEffect(() => {
    if (!session.document) return;
    for (const draft of Object.values(editorDrafts.drafts)) {
      if (draft.status === 'stale') continue;
      const current = session.document.scene.nodes.find((node) => node.id === draft.nodeId);
      if (!current) {
        editorDraftStore.markStale(draft.nodeId, 'Object no longer exists');
      } else if (
        current.content?.type !== draft.baseNode.content?.type
      ) {
        editorDraftStore.markStale(draft.nodeId, 'Object content type changed');
      } else if (draft.baseRevision !== session.revision && draft.status !== 'staged') {
        editorDraftStore.markStale(draft.nodeId, 'Project revision changed');
      }
    }
  }, [editorDraftStore, editorDrafts.drafts, session.document, session.revision]);

  useEffect(() => {
    if (changeSet.status === 'discarded') {
      editorDraftStore.restoreStagedAsDirty();
    }
  }, [changeSet.status, editorDraftStore]);

  useEffect(() => {
    const guard = (event: BeforeUnloadEvent): void => {
      if (!editorDraftStore.hasDirtyDrafts()) return;
      event.preventDefault();
      event.returnValue = '';
    };
    window.addEventListener('beforeunload', guard);
    return () => window.removeEventListener('beforeunload', guard);
  }, [editorDraftStore]);

  const updateEditorValues = (values: EditorValues): void => {
    if (!selected) return;
    editorDraftStore.ensure(selected, session.revision);
    editorDraftStore.commit(selected.id, values);
  };

  const stageDraft = async (): Promise<void> => {
    if (!selected) return;
    const draft = editorDraftStore.snapshot.drafts[selected.id];
    if (!draft || draft.status === 'stale') return;
    try {
      const operations = compileDraftOperations(
        draft.baseNode,
        draft.values,
        selectedDefinition,
      );
      if (operations.length === 0) return;
      await changeSetStore.addOperations(session.revision, operations);
      editorDraftStore.markStaged(selected.id);
    } catch (cause) {
      editorDraftStore.markStageFailed(
        selected.id,
        cause instanceof Error ? cause.message : String(cause),
      );
    }
  };

  const submitAgent = async (text: string): Promise<void> => {
    const camera = viewportAdapter.current?.getCameraContext();
    if (!camera || !session.document) {
      throw new Error('Scene context is unavailable');
    }
    await agentStore.submit({
      text,
      context: {
        projectId: session.projectId,
        revision: session.revision,
        selectionIds: session.selectionIds,
        focusNodeId: selected?.id ?? null,
        camera,
      },
    });
  };

  useEffect(() => {
    const changeSetId = agent.linkedChangeSetId;
    if (!changeSetId) return;
    void changeSetStore.loadLinked(changeSetId)
      .finally(() => setDock('changes'));
  }, [agent.linkedChangeSetId, changeSetStore]);
  const changeSetEntries = changeSet.record?.entries ?? [];
  const diagnostics = mergeDiagnostics(
    session.diagnostics,
    changeSet.active?.diagnostics ?? [],
    changeSet.record?.diagnostics ?? [],
  );
  const selectEvidence = (tab: EvidenceTab) => {
    if (evidenceOpen && evidenceTab === tab) {
      setEvidenceOpen(false);
      return;
    }
    setEvidenceTab(tab);
    setEvidenceOpen(true);
  };
  return (
    <main className="workbench">
      <WorkbenchHeader />
      <section className="workbench__body">
        <nav aria-label="Project navigation" className="objects-panel">
          <ObjectBrowser
            nodes={buildObjectTree(nodes)}
            selectedIds={session.selectionIds}
            onSelect={(id) => sessionStore.select([id])}
          />
        </nav>
        <Viewport
          previewUrl={changeSet.candidateUrl ?? session.previewUrl}
          reference={session.reference}
          selectionIds={session.selectionIds}
          onSelection={(ids) => sessionStore.select(ids)}
          editActive={dock === 'inspect'}
          transformTarget={selected && editorValues
            ? {
                nodeId: selected.id,
                baseTransform: selectedDraft?.stagedValues?.transform
                  ?? selectedDraft?.baseNode.transform
                  ?? identityEditorTransform(),
                transform: editorValues.transform,
              }
            : null}
          transformMode={gizmoMode}
          transformSpace={transformSpace}
          onTransformPreviewStart={() => {
            if (selected) editorDraftStore.beginPreview(selected.id);
          }}
          onTransformPreview={(transform) => {
            if (!selected || !editorValues) return;
            editorDraftStore.updatePreview(selected.id, {
              ...editorValues,
              transform: structuredClone(transform),
            });
          }}
          onTransformPreviewEnd={() => {
            if (selected) editorDraftStore.finishPreview(selected.id);
          }}
          onTransformPreviewCancel={() => {
            if (selected) editorDraftStore.cancelPreview(selected.id);
          }}
          onAdapter={(adapter) => {
            viewportAdapter.current = adapter;
          }}
        />
        <aside aria-label="Review dock" className="review-dock">
          <div role="tablist" aria-label="Review" className="review-dock__tabs">
            {(['agent', 'inspect', 'changes'] as const).map((tab) => (
              <button
                role="tab"
                aria-selected={dock === tab}
                className="review-dock__tab"
                key={tab}
                onClick={() => setDock(tab)}
              >
                {tab === 'agent' ? 'Agent' : tab === 'inspect' ? 'Inspect' : 'Changes'}
              </button>
            ))}
          </div>
          <div className="review-dock__panel" hidden={dock !== 'agent'}>
            <AgentPanel
              store={agentStore}
              projectId={session.projectId}
              revision={session.revision}
              focusName={selected?.name ?? null}
              onOpenManual={() => setDock('inspect')}
              onSubmit={submitAgent}
              onCancel={() => agentStore.cancel()}
              onRetry={() => agentStore.retry()}
              onReviewChanges={() => setDock('changes')}
            />
          </div>
          <div className="review-dock__panel" hidden={dock !== 'inspect'}>
            <ComputePanel status={session.compute} />
            <RouteInspector
              node={selected}
              document={session.document}
              revision={session.revision}
              onSelect={(nodeId) => sessionStore.select([nodeId])}
            />
            {selectedDraft && editorValues && (
              <TransformEditor
                values={editorValues}
                draft={selectedDraft}
                materials={session.document?.materials ?? []}
                mode={gizmoMode}
                space={transformSpace}
                onValues={updateEditorValues}
                onPreviewStart={() => {
                  if (selected) editorDraftStore.beginPreview(selected.id);
                }}
                onMode={setGizmoMode}
                onSpace={setTransformSpace}
                onUndo={() => {
                  if (selected) editorDraftStore.undo(selected.id);
                }}
                onRedo={() => {
                  if (selected) editorDraftStore.redo(selected.id);
                }}
                onStage={() => void stageDraft()}
                onDiscard={() => {
                  if (selected) editorDraftStore.discard(selected.id);
                }}
              />
            )}
            <FormRenderer
              node={selected}
              definition={selectedDefinition}
              values={editorValues}
              onValues={updateEditorValues}
            />
            <div className="review-actions">
              <div className="review-actions__status">
                <span>Change set</span>
                <strong>{changeSet.status}</strong>
              </div>
              {changeSet.impact && (
                <div className="review-actions__impact">
                  <span>Impact</span>
                  <strong>{changeSet.impact.impact.scope}</strong>
                  <span>
                    +{changeSet.impact.impact.sourceNodes.added} / ~
                    {changeSet.impact.impact.sourceNodes.updated} source
                  </span>
                  {changeSet.impact.impact.fullSceneRebuildRequired && (
                    <span className="review-actions__warning">full scene rebuild</span>
                  )}
                </div>
              )}
              {changeSet.active?.spatial && (
                <div className="review-actions__spatial">
                  <span>{changeSet.active.spatial.changedVisibleNodes} visible objects</span>
                  {changeSet.active.spatial.outsideNodeCount > 0 && (
                    <span className="review-actions__warning">
                      {changeSet.active.spatial.outsideNodeCount} outside the scene
                    </span>
                  )}
                </div>
              )}
              <div className="review-actions__buttons">
                <button
                  type="button"
                  className="button button--secondary"
                  disabled={!changeSet.active?.candidateDigest}
                  onClick={() => {
                    const digest = changeSet.active?.candidateDigest;
                    if (digest) changeSetStore.markReviewed(digest);
                  }}
                >
                  Review candidate
                </button>
                <button
                  type="button"
                  className="button button--primary"
                  disabled={!changeSet.canApprove}
                  onClick={async () => {
                    const committedRevision = await changeSetStore.commit();
                    await sessionStore.refresh();
                    if (selected) {
                      editorDraftStore.clear(selected.id);
                      editorDraftStore.ensure(selected, committedRevision);
                    }
                  }}
                >
                  Approve &amp; commit
                </button>
              </div>
            </div>
          </div>
          <div className="review-dock__panel" hidden={dock !== 'changes'}>
            <ChangesPanel
              active={changeSet.active}
              record={changeSet.record}
              status={changeSet.status}
              nodes={nodes}
              error={changeSet.error}
              onRemove={(entryId) => void changeSetStore.removeEntry(entryId)}
            />
          </div>
        </aside>
      </section>
      <section className="workbench__evidence">
        {evidenceOpen && (
          <EvidenceDrawer
            tab={evidenceTab}
            history={session.history}
            diagnostics={diagnostics}
            entries={changeSetEntries}
            nodes={nodes}
            onClose={() => setEvidenceOpen(false)}
            onFocusDiagnostic={(objectId) => {
              sessionStore.select([objectId]);
              setDock('inspect');
            }}
          />
        )}
        <footer role="tablist" aria-label="Evidence" className="evidence-bar">
          {(['operations', 'timeline', 'diagnostics'] as const).map((tab) => (
            <button
              role="tab"
              aria-selected={evidenceOpen && evidenceTab === tab}
              aria-controls="evidence-drawer"
              className="evidence-bar__tab"
              key={tab}
              onClick={() => selectEvidence(tab)}
            >
              {tab[0]!.toUpperCase() + tab.slice(1)}
              {tab === 'operations' && changeSetEntries.length > 0 && (
                <span className="evidence-bar__count">{changeSetEntries.length}</span>
              )}
              {tab === 'diagnostics' && diagnostics.length > 0 && (
                <span className="evidence-bar__count">{diagnostics.length}</span>
              )}
            </button>
          ))}
          <span className="evidence-bar__status">
            {evidenceTab === 'diagnostics'
              ? `${diagnostics.length} diagnostics`
              : evidenceTab === 'timeline'
                ? `Current revision ${session.revision}`
                : changeSet.status}
          </span>
        </footer>
      </section>
    </main>
  );
}

function identityEditorTransform(): StudioTransform {
  return {
    translation: { x: 0, y: 0, z: 0 },
    rotation: [0, 0, 0, 1],
    scale: { x: 1, y: 1, z: 1 },
  };
}
