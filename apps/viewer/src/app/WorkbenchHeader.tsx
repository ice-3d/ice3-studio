import { useSession } from '../session/SessionProvider';

export function WorkbenchHeader() {
  const session = useSession();
  return (
    <header className="workbench__header">
      <div className="brand-lockup">
        <span className="brand-lockup__mark" aria-hidden="true">I3</span>
        <div>
          <strong>ICE3 Studio</strong>
          <span>Authoring workspace</span>
        </div>
      </div>
      <div className="project-context">
        <span>Project</span>
        <h1>{session.projectId || 'Loading project'}</h1>
      </div>
      <div className="workbench__status">
        <span className={`status-dot status-dot--${session.connection}`} aria-hidden="true" />
        <span role="status">Revision {session.revision}</span>
        <span className="connection-label">{session.connection}</span>
      </div>
    </header>
  );
}
