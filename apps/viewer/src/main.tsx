import { createRoot } from 'react-dom/client';
import { Workbench } from './app/Workbench';
import { StudioClient } from './api/StudioClient';
import { SessionProvider } from './session/SessionProvider';
import { SessionStore } from './session/SessionStore';
import { ChangeSetProvider } from './changeset/ChangeSetProvider';
import { ChangeSetStore } from './changeset/ChangeSetStore';

const root = document.getElementById('app');
if (!root) throw new Error('Workbench root is missing');
const client = new StudioClient();
const session = new SessionStore(client);
const changeSet = new ChangeSetStore(client);
client.subscribe((event) => {
  if (event.event === 'revision.committed') void session.refresh();
});
void session.refresh();
createRoot(root).render(
  <SessionProvider store={session}>
    <ChangeSetProvider store={changeSet}>
      <Workbench />
    </ChangeSetProvider>
  </SessionProvider>,
);
