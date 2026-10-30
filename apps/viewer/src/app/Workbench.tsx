import './studioTokens.css';

export function Workbench() {
  return (
    <main className="workbench">
      <header className="workbench__header">
        <strong>ICE3 / STUDIO</strong>
        <span>Connecting</span>
      </header>
      <section className="workbench__body">
        <nav aria-label="Project navigation">Navigation</nav>
        <section aria-label="Model viewport">Viewport</section>
        <aside aria-label="Review dock">Review</aside>
      </section>
      <footer>Evidence</footer>
    </main>
  );
}
