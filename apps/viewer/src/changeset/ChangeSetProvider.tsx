import { createContext, useContext, useSyncExternalStore, type ReactNode } from 'react';
import type { ChangeSetSnapshot, ChangeSetStore } from './ChangeSetStore';

const Context = createContext<ChangeSetStore | null>(null);

export function ChangeSetProvider({ store, children }: { store: ChangeSetStore; children: ReactNode }) {
  return <Context.Provider value={store}>{children}</Context.Provider>;
}

export function useChangeSetStore(): ChangeSetStore {
  const store = useContext(Context);
  if (!store) throw new Error('ChangeSetProvider is missing');
  return store;
}

export function useChangeSet(): ChangeSetSnapshot {
  const store = useChangeSetStore();
  return useSyncExternalStore(store.subscribe, () => store.snapshot);
}
