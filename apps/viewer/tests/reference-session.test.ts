import assert from 'node:assert/strict';
import test from 'node:test';
import { SessionStore } from '../src/session/SessionStore';
import {
  lightenReferenceColor,
  mapReferenceSelection,
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
