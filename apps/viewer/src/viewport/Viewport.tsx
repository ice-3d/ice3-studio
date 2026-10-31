import { useEffect, useRef } from 'react';
import type { StudioReferenceSession } from '../session/SessionStore';
import { SceneAdapter } from './SceneAdapter';

type Props = {
  previewUrl: string | null;
  reference: StudioReferenceSession | null;
  selectionIds: readonly string[];
  onSelection: (ids: readonly string[]) => void;
};

export function Viewport({ previewUrl, reference, selectionIds, onSelection }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const scene = useRef<SceneAdapter | null>(null);

  useEffect(() => {
    const adapter = new SceneAdapter();
    adapter.onSelection = onSelection;
    scene.current = adapter;
    if (host.current) void adapter.mount(host.current);
    return () => {
      scene.current = null;
    };
  }, []);

  useEffect(() => {
    if (reference) {
      void scene.current?.showReference(reference.asset, reference.bindings);
    } else if (previewUrl) {
      void scene.current?.show(previewUrl);
    }
  }, [previewUrl, reference]);

  useEffect(() => {
    scene.current?.select(selectionIds);
  }, [selectionIds]);

  return <section className="viewport" ref={host} aria-label="Model viewport" />;
}
