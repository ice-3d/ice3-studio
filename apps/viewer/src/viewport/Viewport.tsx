import { useEffect, useRef } from 'react';
import type { StudioReferenceSession } from '../session/SessionStore';
import { ExplodeTool } from './ExplodeTool';
import { MeasureTool } from './MeasureTool';
import { SectionTool } from './SectionTool';
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
    const tool = host.current ? new MeasureTool(host.current, adapter) : null;
    const section = host.current ? new SectionTool(host.current, adapter) : null;
    const explode = host.current ? new ExplodeTool(host.current, adapter) : null;
    return () => {
      tool?.dispose();
      section?.dispose();
      explode?.dispose();
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
