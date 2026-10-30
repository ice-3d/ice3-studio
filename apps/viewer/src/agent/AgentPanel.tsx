import { useState } from 'react';
import { AgentStore } from './AgentStore';

const unavailableStore = new AgentStore({
  connection: 'unavailable',
  submitIntent: async () => ({ runId: '' }),
  cancel: async () => {},
  subscribe: () => () => {},
});

export function AgentPanel({ store = unavailableStore }: { store?: AgentStore }) {
  const [text, setText] = useState('');
  return (
    <section aria-label="Agent">
      <h2>Agent</h2>
      {store.snapshot.connection === 'unavailable' ? (
        <p>The Agent Gateway is not connected. Model review and form editing remain available.</p>
      ) : (
        <form onSubmit={(event) => {
          event.preventDefault();
          void store.submit(text);
        }}>
          <input value={text} onChange={(event) => setText(event.target.value)} />
        </form>
      )}
    </section>
  );
}
