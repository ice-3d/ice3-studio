import './studioTokens.css';
import { WorkbenchHeader } from './WorkbenchHeader';
import { useSession, useSessionStore } from '../session/SessionProvider';
import { buildObjectTree } from '../navigation/objectTreeModel';
import { ObjectTree } from '../navigation/ObjectTree';
import { Viewport } from '../viewport/Viewport';
import { FormRenderer } from '../forms/FormRenderer';
import { useChangeSet, useChangeSetStore } from '../changeset/ChangeSetProvider';
import { AgentPanel } from '../agent/AgentPanel';
import { useState } from 'react';
import { ToolchainPanel } from '../toolchain/ToolchainPanel';

export function Workbench() {
  const session = useSession();
  const sessionStore = useSessionStore();
  const changeSetStore = useChangeSetStore();
  const changeSet = useChangeSet();
  const [dock, setDock] = useState<'agent' | 'inspect' | 'changes'>('inspect');
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
        <nav aria-label="Project navigation">
          <h2>Objects</h2>
          <ObjectTree
            nodes={buildObjectTree(nodes)}
            selectedIds={session.selectionIds}
            onSelect={(id) => sessionStore.select([id])}
          />
        </nav>
        <Viewport
          previewUrl={changeSet.candidateUrl ?? session.previewUrl}
          onSelection={(ids) => sessionStore.select(ids)}
        />
        <aside aria-label="Review dock">
          <div role="tablist" aria-label="Review">
            {(['agent', 'inspect', 'changes'] as const).map((tab) => (
              <button
                role="tab"
                aria-selected={dock === tab}
                key={tab}
                onClick={() => setDock(tab)}
              >
                {tab === 'agent' ? 'Agent' : tab === 'inspect' ? 'Inspect' : 'Changes'}
              </button>
            ))}
          </div>
          <div hidden={dock !== 'agent'}><AgentPanel /></div>
          <div hidden={dock !== 'inspect'}>
            <ToolchainPanel status={session.toolchain} />
            <FormRenderer
              node={selected}
              definition={selectedDefinition}
              onStage={async (operations) => {
                await changeSetStore.addOperations(session.revision, operations);
              }}
            />
            <p>{changeSet.status}</p>
            <button
              type="button"
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
              disabled={!changeSet.canApprove}
              onClick={async () => {
                await changeSetStore.commit();
                await sessionStore.refresh();
              }}
            >
              Approve &amp; commit
            </button>
          </div>
          <div hidden={dock !== 'changes'}>
            <h2>Changes</h2>
            <p>{changeSet.active ? `${changeSet.active.diff.updatedNodeIds.length} updated objects` : 'No staged changes'}</p>
          </div>
        </aside>
      </section>
      <footer role="tablist" aria-label="Evidence">
        {(['operations', 'timeline', 'diagnostics'] as const).map((tab) => (
          <button
            role="tab"
            aria-selected={drawer === tab}
            key={tab}
            onClick={() => setDrawer(tab)}
          >
            {tab[0]!.toUpperCase() + tab.slice(1)}
          </button>
        ))}
        <span>
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
