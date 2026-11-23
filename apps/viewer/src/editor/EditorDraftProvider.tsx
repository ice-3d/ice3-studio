import {
  createContext,
  useContext,
  useSyncExternalStore,
  type ReactNode,
} from 'react';
import type { EditorDraftSnapshot, EditorDraftStore } from './EditorDraftStore';

const Context = createContext<EditorDraftStore | null>(null);

export function EditorDraftProvider({
  store,
  children,
}: {
  store: EditorDraftStore;
  children: ReactNode;
}) {
  return <Context.Provider value={store}>{children}</Context.Provider>;
}

export function useEditorDraftStore(): EditorDraftStore {
  const store = useContext(Context);
  if (!store) throw new Error('EditorDraftProvider is missing');
  return store;
}

export function useEditorDrafts(): EditorDraftSnapshot {
  const store = useEditorDraftStore();
  return useSyncExternalStore(store.subscribe, () => store.snapshot);
}
