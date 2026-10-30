import './studioTokens.css';
import { WorkbenchHeader } from './WorkbenchHeader';
import { useSession, useSessionStore } from '../session/SessionProvider';
import { buildObjectTree } from '../navigation/objectTreeModel';
import { ObjectTree } from '../navigation/ObjectTree';
import { Viewport } from '../viewport/Viewport';

export function Workbench() {
  const session = useSession();
  const sessionStore = useSessionStore();
  const nodes = session.document?.scene.nodes ?? [];
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
          previewUrl={session.previewUrl}
          onSelection={(ids) => sessionStore.select(ids)}
        />
        <aside aria-label="Review dock">
          <h2>Review</h2>
          <p>{session.connection}</p>
        </aside>
      </section>
      <footer>{session.diagnostics.length} diagnostics</footer>
    </main>
  );
}
