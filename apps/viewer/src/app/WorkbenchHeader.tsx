import { useSession } from '../session/SessionProvider';

export function WorkbenchHeader() {
  const session = useSession();
  return (
    <header className="workbench__header">
      <strong>ICE3 / STUDIO</strong>
      <h1>{session.projectId || 'Loading project'}</h1>
      <span role="status">Revision {session.revision}</span>
    </header>
  );
}
