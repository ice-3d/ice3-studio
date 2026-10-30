import { useSession } from '../session/SessionProvider';

export function WorkbenchHeader() {
  const session = useSession();
  return (
    <header className="workbench__header">
      <strong>ICE3 / STUDIO</strong>
      <span>{session.projectId || 'Loading project'} · Revision {session.revision}</span>
    </header>
  );
}
