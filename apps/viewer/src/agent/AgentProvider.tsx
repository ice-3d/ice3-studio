import {
  createContext,
  useContext,
  useSyncExternalStore,
  type ReactNode,
} from 'react';
import type { AgentSnapshot, AgentStore } from './AgentStore';

const Context = createContext<AgentStore | null>(null);

export function AgentProvider({
  store,
  children,
}: {
  store: AgentStore;
  children: ReactNode;
}) {
  return <Context.Provider value={store}>{children}</Context.Provider>;
}

export function useAgentStore(): AgentStore {
  const store = useContext(Context);
  if (!store) throw new Error('AgentProvider is missing');
  return store;
}

export function useAgent(): AgentSnapshot {
  const store = useAgentStore();
  return useSyncExternalStore(store.subscribe, () => store.snapshot);
}
