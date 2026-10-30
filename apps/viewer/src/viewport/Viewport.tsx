import { useEffect, useRef } from 'react';
import { SceneAdapter } from './SceneAdapter';

type Props = {
  previewUrl: string | null;
  onSelection: (ids: readonly string[]) => void;
};

export function Viewport({ previewUrl, onSelection }: Props) {
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
    if (previewUrl) void scene.current?.show(previewUrl);
  }, [previewUrl]);

  return <section className="viewport" ref={host} aria-label="Model viewport" />;
}
