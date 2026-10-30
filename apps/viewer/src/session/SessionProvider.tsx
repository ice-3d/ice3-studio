import { createContext, useContext, useSyncExternalStore, type ReactNode } from 'react';
import type { SessionSnapshot, SessionStore } from './SessionStore';

const SessionContext = createContext<SessionStore | null>(null);

export function SessionProvider({ store, children }: { store: SessionStore; children: ReactNode }) {
  return <SessionContext.Provider value={store}>{children}</SessionContext.Provider>;
}

export function useSession(): SessionSnapshot {
  const store = useSessionStore();
  return useSyncExternalStore(store.subscribe, () => store.snapshot);
}

export function useSessionStore(): SessionStore {
  const store = useContext(SessionContext);
  if (!store) throw new Error('SessionProvider is missing');
  return store;
}
