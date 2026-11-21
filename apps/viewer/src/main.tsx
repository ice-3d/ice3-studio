import { createRoot } from 'react-dom/client';
import { Workbench } from './app/Workbench';
import { StudioClient } from './api/StudioClient';
import { SessionProvider } from './session/SessionProvider';
import { SessionStore } from './session/SessionStore';
import { ChangeSetProvider } from './changeset/ChangeSetProvider';
import { ChangeSetStore } from './changeset/ChangeSetStore';
import { RouteInspectionProvider } from './route/RouteInspectionProvider';
import { RouteInspectionStore } from './route/RouteInspectionStore';

const root = document.getElementById('app');
if (!root) throw new Error('Workbench root is missing');
const client = new StudioClient();
const session = new SessionStore(client);
const changeSet = new ChangeSetStore(client);
const routeInspection = new RouteInspectionStore(client);
client.subscribe((event) => {
  if (event.event === 'revision.committed') void session.refresh();
});
void session.refresh();
createRoot(root).render(
  <SessionProvider store={session}>
    <RouteInspectionProvider store={routeInspection}>
      <ChangeSetProvider store={changeSet}>
        <Workbench />
      </ChangeSetProvider>
    </RouteInspectionProvider>
  </SessionProvider>,
);
