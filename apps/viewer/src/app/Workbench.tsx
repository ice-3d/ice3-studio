import './studioTokens.css';
import { WorkbenchHeader } from './WorkbenchHeader';
import { useSession } from '../session/SessionProvider';

export function Workbench() {
  const session = useSession();
  const nodes = session.document?.scene.nodes ?? [];
  return (
    <main className="workbench">
      <WorkbenchHeader />
      <section className="workbench__body">
        <nav aria-label="Project navigation">
          <h2>Objects</h2>
          <ol>{nodes.map((node) => <li key={node.id}>{node.name}</li>)}</ol>
        </nav>
        <section aria-label="Model viewport">Viewport</section>
        <aside aria-label="Review dock">
          <h2>Review</h2>
          <p>{session.connection}</p>
        </aside>
      </section>
      <footer>{session.diagnostics.length} diagnostics</footer>
    </main>
  );
}
