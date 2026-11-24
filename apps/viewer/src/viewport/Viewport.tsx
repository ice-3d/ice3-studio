import { useEffect, useRef } from 'react';
import type { StudioTransform } from '../api/StudioClient';
import type { GizmoMode, TransformSpace } from '../editor/types';
import type { StudioReferenceSession } from '../session/SessionStore';
import { ExplodeTool } from './ExplodeTool';
import { MeasureTool } from './MeasureTool';
import { SectionTool } from './SectionTool';
import { SceneAdapter } from './SceneAdapter';
import { TransformGizmo } from './TransformGizmo';

type Props = {
  previewUrl: string | null;
  reference: StudioReferenceSession | null;
  selectionIds: readonly string[];
  onSelection: (ids: readonly string[]) => void;
  editActive?: boolean;
  transformTarget?: {
    nodeId: string;
    baseTransform: StudioTransform;
    transform: StudioTransform;
  } | null;
  transformMode?: GizmoMode;
  transformSpace?: TransformSpace;
  onTransformPreviewStart?: () => void;
  onTransformPreview?: (transform: StudioTransform) => void;
  onTransformPreviewEnd?: (transform: StudioTransform) => void;
  onTransformPreviewCancel?: () => void;
};

export function Viewport({
  previewUrl,
  reference,
  selectionIds,
  onSelection,
  editActive = false,
  transformTarget = null,
  transformMode = 'move',
  transformSpace = 'local',
  onTransformPreviewStart,
  onTransformPreview,
  onTransformPreviewEnd,
  onTransformPreviewCancel,
}: Props) {
  const host = useRef<HTMLDivElement>(null);
  const scene = useRef<SceneAdapter | null>(null);
  const gizmo = useRef<TransformGizmo | null>(null);
  const callbacks = useRef({
    onTransformPreviewStart,
    onTransformPreview,
    onTransformPreviewEnd,
    onTransformPreviewCancel,
  });
  callbacks.current = {
    onTransformPreviewStart,
    onTransformPreview,
    onTransformPreviewEnd,
    onTransformPreviewCancel,
  };
  useEffect(() => {
    const adapter = new SceneAdapter();
    adapter.onSelection = onSelection;
    scene.current = adapter;
    if (host.current) {
      void adapter.mount(host.current);
    }
    const tool = host.current ? new MeasureTool(host.current, adapter) : null;
    const section = host.current ? new SectionTool(host.current, adapter) : null;
    const explode = host.current ? new ExplodeTool(host.current, adapter) : null;
    const transformGizmo = host.current
      ? new TransformGizmo(host.current, adapter, {
          beginPreview: () => callbacks.current.onTransformPreviewStart?.(),
          preview: (transform) => callbacks.current.onTransformPreview?.(transform),
          finishPreview: (transform) => callbacks.current.onTransformPreviewEnd?.(transform),
          cancelPreview: () => callbacks.current.onTransformPreviewCancel?.(),
        })
      : null;
    gizmo.current = transformGizmo;
    return () => {
      tool?.dispose();
      section?.dispose();
      explode?.dispose();
      transformGizmo?.dispose();
      gizmo.current = null;
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

  useEffect(() => {
    gizmo.current?.setActive(editActive && Boolean(transformTarget));
    gizmo.current?.setTarget(
      transformTarget?.nodeId ?? null,
      transformTarget?.baseTransform ?? null,
      transformTarget?.transform ?? null,
    );
  }, [editActive, transformTarget]);

  useEffect(() => {
    gizmo.current?.setMode(transformMode);
  }, [transformMode]);

  useEffect(() => {
    gizmo.current?.setSpace(transformSpace);
  }, [transformSpace]);

  return <section className="viewport" ref={host} aria-label="Model viewport" />;
}
