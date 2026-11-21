import { createContext, useContext, useSyncExternalStore, type ReactNode } from 'react';
import type {
  RouteInspectionSnapshot,
  RouteInspectionStore,
} from './RouteInspectionStore';

const RouteInspectionContext = createContext<RouteInspectionStore | null>(null);

export function RouteInspectionProvider({
  store,
  children,
}: {
  store: RouteInspectionStore;
  children: ReactNode;
}) {
  return (
    <RouteInspectionContext.Provider value={store}>
      {children}
    </RouteInspectionContext.Provider>
  );
}

export function useRouteInspectionStore(): RouteInspectionStore {
  const store = useContext(RouteInspectionContext);
  if (!store) throw new Error('RouteInspectionProvider is missing');
  return store;
}

export function useRouteInspection(): RouteInspectionSnapshot {
  const store = useRouteInspectionStore();
  return useSyncExternalStore(store.subscribe, () => store.snapshot);
}
