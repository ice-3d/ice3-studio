import assert from 'node:assert/strict';
import test from 'node:test';
import { SessionStore } from '../src/session/SessionStore';
import {
  applyReferenceOverrides,
  lightenReferenceColor,
  mapReferenceSelection,
  referenceSurfaceColor,
} from '../src/viewport/SceneAdapter';

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

test('maps infrastructure and pipe colors by real-world material role', () => {
  assert.equal(referenceSurfaceColor(['Ground'], '#1a1b17'), '#8da478');
  assert.equal(referenceSurfaceColor(['Ground', 'Ground.Slab'], '#1a1b17'), '#91a77c');
  assert.equal(referenceSurfaceColor(['RoadNetwork', 'RoadNetwork.MainAccess'], '#1f2121'), '#25282b');
  assert.equal(referenceSurfaceColor(['RoadNetwork', 'RoadNetwork.MainAccess.Curb.N'], '#a39e87'), '#c9c7be');
  assert.equal(referenceSurfaceColor(['RoadNetwork', 'RoadNetwork.StormDrain.00'], '#2e383b'), '#454a4c');
  assert.equal(referenceSurfaceColor(['ProcessPipes', 'Pipe.air-01'], '#7a8c8f'), '#3f8fc5');
  assert.equal(referenceSurfaceColor(['ProcessPipes', 'Pipe.gas-01'], '#63784d'), '#d8b82e');
  assert.equal(referenceSurfaceColor(['ProcessPipes', 'Pipe.chemical-01a'], '#80754d'), '#d4862d');
  assert.equal(referenceSurfaceColor(['ProcessPipes', 'Pipe.sludge-01'], '#695980'), '#8a6249');
  assert.equal(referenceSurfaceColor(['ProcessPipes', 'Pipe.loop-a-01'], '#477063'), '#3f8b69');
  assert.equal(referenceSurfaceColor(['ProcessPipes', 'Pipe.filtrate-01'], '#695980'), '#7565a8');
  assert.equal(referenceSurfaceColor(['ProcessPipes', 'Pipe.main-01-IN-001-GR-101'], '#1f6b94'), '#347fae');
  assert.equal(
    referenceSurfaceColor(['Buildings', 'Operations Center'], '#263d45'),
    '#4b7b8d',
  );
  assert.equal(referenceSurfaceColor(['AE-101'], '#265c66'), '#1f6675');
  assert.equal(referenceSurfaceColor(['AE-101'], '#66635c'), '#d6d7d2');
  assert.equal(referenceSurfaceColor(['AE-101'], '#6e7a7d'), '#a8b1b5');
  assert.equal(referenceSurfaceColor(['AE-101'], '#ced4d5'), '#2f6f78');
});

test('inherits equipment color through mesh children without tinting shared geometry', () => {
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
  assert.equal(mixed.material?.color, lightenReferenceColor('#334455'));
});

test('removes the legacy external stair primitive from facilities', () => {
  const children = Array.from({ length: 7 }, (_, index) => (
    fakeNode(`site/facility/part-${index}`, { color: '#66635c' })
  ));
  const building = fakeNode('Site.Facility.FAC-CTRL-101', null, children);
  applyReferenceOverrides(
    fakeNode('Buildings', null, [building]),
    [],
    new WeakMap(),
  );
  assert.equal(children[6]?.visible, false);
  assert.equal(children[5]?.visible, true);
});

test('hides legacy paving blocks so the grass base remains continuous', () => {
  const pad = fakeNode('SitePads.00', { color: '#66635c' });
  const ground = fakeNode('Ground', null, [pad]);
  applyReferenceOverrides(
    fakeNode('Site', null, [ground]),
    [],
    new WeakMap(),
  );
  assert.equal(pad.visible, false);
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
        equipment: 204,
      },
      importedAt: '2026-10-31T00:00:00Z',
    },
    tilesetUrl: '/reference-assets/asset-000001/tileset.json',
    preparedPreviewUrl: '/reference-assets/asset-000001/prepared.glb',
    equipmentManifestUrl: '/reference-assets/asset-000001/equipment.json',
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
