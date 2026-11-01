import assert from 'node:assert/strict';
import test from 'node:test';
import { SessionStore } from '../src/session/SessionStore';
import {
  applyReferenceOverrides,
  lightenReferenceColor,
  mapReferenceSelection,
  SCENE_BACKGROUND_COLOR,
} from '../src/viewport/SceneAdapter';

test('keeps the dark workbench background while lighting the scene', () => {
  assert.equal(SCENE_BACKGROUND_COLOR, '#14181c');
});

test('loads reference metadata without requesting document geometry', async () => {
  const source = {
    document: async () => ({
      projectId: 'reference-session',
      revision: 7,
      scene: {
        nodes: [{
          id: 'node-000001',
          name: 'AE-101',
          parentId: null,
          visible: true,
          transform: {
            translation: { x: 4, y: 0, z: 0 },
            rotation: [0, 0, 0, 1] as [number, number, number, number],
            scale: { x: 1, y: 1, z: 1 },
          },
          metadata: {},
          materialId: 'material-000001',
          content: {
            type: 'reference_instance',
            assetId: 'asset-000001',
            selectionId: 'ice3-virtual-wwtp/equipment/AE-101',
          },
        }],
      },
      materials: [{
        id: 'material-000001',
        name: 'Override',
        baseColor: [0.4, 0.6, 0.8, 1] as [number, number, number, number],
      }],
      components: [],
    }),
    validation: async () => [],
    referenceAsset: async () => largeReferenceDescriptor(),
    referenceBindings: async () => ({
      items: [{
        selectionId: 'ice3-virtual-wwtp/equipment/AE-101',
        name: '好氧池 A',
      }],
      nextCursor: null,
    }),
  };
  const store = new SessionStore(source);
  await store.refresh();

  assert.equal(store.snapshot.document?.scene.nodes.length, 1);
  assert.equal(store.snapshot.reference?.asset.descriptor.stats.effectiveTriangles, 5_757_962);
  assert.equal(store.snapshot.reference?.bindings[0]?.nodeId, 'node-000001');
  assert.deepEqual(
    store.snapshot.reference?.bindings[0]?.transform,
    { translation: { x: 4, y: 0, z: 0 }, rotation: [0, 0, 0, 1], scale: { x: 1, y: 1, z: 1 } },
  );
  assert.equal(store.snapshot.reference?.bindings[0]?.materialColor, '#6699cc');
});

test('maps imported selection IDs back to Studio binding IDs', () => {
  assert.equal(mapReferenceSelection(
    'ice3-virtual-wwtp/equipment/AE-101',
    [{ nodeId: 'node-000001', selectionId: 'ice3-virtual-wwtp/equipment/AE-101' }],
  ), 'node-000001');
});

test('lifts reference materials without discarding their hue', () => {
  assert.equal(lightenReferenceColor('#334455'), '#89939c');
  assert.equal(lightenReferenceColor('#336699cc'), '#89a6c4cc');
  assert.equal(lightenReferenceColor('invalid'), 'invalid');
});

test('keeps the asset material when a binding declares no slot', () => {
  const node = fakeNode('site/opaque', { color: '#4a5560' });
  applyReferenceOverrides(fakeNode('site/root', null, [node]), [], new WeakMap());
  assert.equal(node.material?.color, '#4a5560');
});

test('inherits binding color through mesh children without tinting shared geometry', () => {
  const material = (color: string) => ({ color });
  const child = fakeNode('site/child', material('#334455'));
  const parent = fakeNode('site/equipment/AE-101', null, [child]);
  const mixed = fakeNode('site/shared', material('#334455'));
  mixed.semanticSelectionIds = ['site/equipment/AE-101', 'site/equipment/SC-101'];
  const scene = fakeNode('site/root', null, [parent, mixed]);
  applyReferenceOverrides(scene, [
    {
      selectionId: 'site/equipment/AE-101',
      name: 'AE-101',
      nodeId: 'node-ae-101',
      transform: identityTransform(),
      visible: true,
      materialId: 'material-aerobic',
      materialColor: '#26b8c6',
    },
    {
      selectionId: 'site/equipment/SC-101',
      name: 'SC-101',
      nodeId: 'node-sc-101',
      transform: identityTransform(),
      visible: true,
      materialId: 'material-secondary',
      materialColor: '#82d8e8',
    },
  ], new WeakMap());

  assert.equal(child.material?.color, '#26b8c6');
  // 共享几何同时属于两个绑定，无法判定用哪个颜色，因此保持资产原色。
  assert.equal(mixed.material?.color, '#334455');
});

test('hides only the bindings the asset marked hidden', () => {
  const obsolete = fakeNode('site/part-obsolete', { color: '#66635c' });
  const kept = fakeNode('site/part-kept', { color: '#66635c' });
  const assembly = fakeNode('Asset.Assembly', null, [obsolete, kept]);
  applyReferenceOverrides(fakeNode('Asset', null, [assembly]), [
    {
      selectionId: 'site/part-obsolete',
      name: 'part-obsolete',
      nodeId: 'node-obsolete',
      transform: identityTransform(),
      visible: true,
      materialId: null,
      materialColor: null,
      hidden: true,
    },
  ], new WeakMap());
  assert.equal(obsolete.visible, false);
  assert.equal(kept.visible, true);
});

function largeReferenceDescriptor() {
  return {
    descriptor: {
      assetId: 'asset-000001',
      sourceDigest: 'a'.repeat(64),
      preparedDigest: 'b'.repeat(64),
      tilesetDigest: 'c'.repeat(64),
      bounds: {
        min: { x: -625, y: -1, z: -390 },
        max: { x: 625, y: 31, z: 390 },
      },
      stats: {
        sourceNodes: 1761,
        effectiveTriangles: 5_757_962,
        uniqueMeshes: 1632,
        materials: 75,
        bindings: 204,
      },
      materialSlots: [
        { id: 'tank-aerated', color: '#26b8c6', role: 'surface' },
        { id: 'pipe-sludge', color: '#8a6249', role: 'pipe' },
      ],
      importedAt: '2026-10-31T00:00:00Z',
    },
    tilesetUrl: '/reference-assets/asset-000001/tileset.json',
    preparedPreviewUrl: '/reference-assets/asset-000001/prepared.glb',
    bindingManifestUrl: '/reference-assets/asset-000001/bindings.json',
  };
}

function fakeNode(
  selectionId: string,
  material: { color: string } | null,
  children: FakeNode[] = [],
): FakeNode {
  return {
    name: selectionId,
    selectionId,
    semanticSelectionIds: [] as string[],
    visible: true,
    position: { x: 0, y: 0, z: 0 },
    rotation: { x: 0, y: 0, z: 0 },
    scale: { x: 1, y: 1, z: 1 },
    ...(material ? { material } : {}),
    children,
    setLocalMatrix: () => undefined,
  };
}

type FakeNode = {
  name: string;
  selectionId: string;
  semanticSelectionIds: string[];
  visible: boolean;
  position: { x: number; y: number; z: number };
  rotation: { x: number; y: number; z: number };
  scale: { x: number; y: number; z: number };
  material?: { color: string };
  children: FakeNode[];
  setLocalMatrix: () => undefined;
};

function identityTransform() {
  return {
    translation: { x: 0, y: 0, z: 0 },
    rotation: [0, 0, 0, 1] as [number, number, number, number],
    scale: { x: 1, y: 1, z: 1 },
  };
}
