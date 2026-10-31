import assert from 'node:assert/strict';
import test from 'node:test';
import { SessionStore } from '../src/session/SessionStore';
import { mapReferenceSelection } from '../src/viewport/SceneAdapter';

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
          content: {
            type: 'reference_instance',
            assetId: 'asset-000001',
            selectionId: 'ice3-virtual-wwtp/equipment/AE-101',
          },
        }],
      },
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
});

test('maps imported selection IDs back to Studio binding IDs', () => {
  assert.equal(mapReferenceSelection(
    'ice3-virtual-wwtp/equipment/AE-101',
    [{ nodeId: 'node-000001', selectionId: 'ice3-virtual-wwtp/equipment/AE-101' }],
  ), 'node-000001');
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
