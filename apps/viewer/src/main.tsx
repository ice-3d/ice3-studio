import { createRoot } from 'react-dom/client';
import { Workbench } from './app/Workbench';
import { StudioClient } from './api/StudioClient';
import { SessionProvider } from './session/SessionProvider';
import { SessionStore } from './session/SessionStore';

const root = document.getElementById('app');
if (!root) throw new Error('Workbench root is missing');
const client = new StudioClient();
const session = new SessionStore(client);
client.subscribe((event) => {
  if (event.event === 'revision.committed') void session.refresh();
});
void session.refresh();
createRoot(root).render(
  <SessionProvider store={session}>
    <Workbench />
  </SessionProvider>,
);
