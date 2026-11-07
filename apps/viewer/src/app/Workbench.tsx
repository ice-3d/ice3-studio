import './studioTokens.css';
import { WorkbenchHeader } from './WorkbenchHeader';
import { useSession, useSessionStore } from '../session/SessionProvider';
import { buildObjectTree } from '../navigation/objectTreeModel';
import { ObjectBrowser } from '../navigation/ObjectBrowser';
import { Viewport } from '../viewport/Viewport';
import { FormRenderer } from '../forms/FormRenderer';
import { useChangeSet, useChangeSetStore } from '../changeset/ChangeSetProvider';
import { AgentPanel } from '../agent/AgentPanel';
import { useState } from 'react';
import { ComputePanel } from '../compute/ComputePanel';

export function Workbench() {
  const session = useSession();
  const sessionStore = useSessionStore();
  const changeSetStore = useChangeSetStore();
  const changeSet = useChangeSet();
  const [dock, setDock] = useState<'agent' | 'inspect' | 'changes'>('agent');
  const [drawer, setDrawer] = useState<'operations' | 'timeline' | 'diagnostics'>('operations');
  const nodes = session.document?.scene.nodes ?? [];
  const selected = nodes.find((node) => node.id === session.selectionIds[0]);
  const definitionId = typeof selected?.content?.definitionId === 'string'
    ? selected.content.definitionId
    : null;
  const selectedDefinition = definitionId
    ? session.document?.components.find((definition) => definition.id === definitionId)
    : undefined;
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
              projectId={session.projectId}
              revision={session.revision}
              focusName={selected?.name ?? null}
              onOpenManual={() => setDock('inspect')}
            />
          </div>
          <div className="review-dock__panel" hidden={dock !== 'inspect'}>
            <ComputePanel status={session.compute} />
            <FormRenderer
              node={selected}
              definition={selectedDefinition}
              onStage={async (operations) => {
                await changeSetStore.addOperations(session.revision, operations);
              }}
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
                    await changeSetStore.commit();
                    await sessionStore.refresh();
                  }}
                >
                  Approve &amp; commit
                </button>
              </div>
            </div>
          </div>
          <div className="review-dock__panel" hidden={dock !== 'changes'}>
            <span className="panel-kicker">Current proposal</span>
            <h2>Changes</h2>
            <p>{changeSet.active ? `${changeSet.active.diff.updatedNodeIds.length} updated objects` : 'No staged changes'}</p>
          </div>
        </aside>
      </section>
      <footer role="tablist" aria-label="Evidence" className="evidence-bar">
        {(['operations', 'timeline', 'diagnostics'] as const).map((tab) => (
          <button
            role="tab"
            aria-selected={drawer === tab}
            className="evidence-bar__tab"
            key={tab}
            onClick={() => setDrawer(tab)}
          >
            {tab[0]!.toUpperCase() + tab.slice(1)}
          </button>
        ))}
        <span className="evidence-bar__status">
          {drawer === 'diagnostics'
            ? `${session.diagnostics.length} diagnostics`
            : drawer === 'timeline'
              ? `Current revision ${session.revision}`
              : changeSet.status}
        </span>
      </footer>
    </main>
  );
}
