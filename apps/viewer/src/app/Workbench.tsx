import './studioTokens.css';
import { WorkbenchHeader } from './WorkbenchHeader';
import { useSession, useSessionStore } from '../session/SessionProvider';
import { buildObjectTree } from '../navigation/objectTreeModel';
import { ObjectTree } from '../navigation/ObjectTree';
import { Viewport } from '../viewport/Viewport';
import { FormRenderer } from '../forms/FormRenderer';
import { useChangeSet, useChangeSetStore } from '../changeset/ChangeSetProvider';
import { AgentPanel } from '../agent/AgentPanel';

export function Workbench() {
  const session = useSession();
  const sessionStore = useSessionStore();
  const changeSetStore = useChangeSetStore();
  const changeSet = useChangeSet();
  const nodes = session.document?.scene.nodes ?? [];
  const selected = nodes.find((node) => node.id === session.selectionIds[0]);
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
          <AgentPanel />
          <h2>Inspect</h2>
          <FormRenderer
            node={selected}
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
          <p>{session.connection}</p>
        </aside>
      </section>
      <footer>{session.diagnostics.length} diagnostics</footer>
    </main>
  );
}
