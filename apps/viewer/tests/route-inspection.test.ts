import assert from 'node:assert/strict';
import test from 'node:test';
import type {
  StudioDocument,
  StudioNode,
  StudioRouteInspection,
} from '../src/api/StudioClient';
import { routeLineIdForNode } from '../src/route/routeInspectionModel';
import { RouteInspectionStore } from '../src/route/RouteInspectionStore';

function inspection(lineId: string): StudioRouteInspection {
  return {
    revision: 2,
    lineId,
    supportLineIds: [],
    segments: [],
    attachments: [],
    supports: [],
    truncated: {
      segments: false,
      points: false,
      attachments: false,
      supports: false,
    },
  };
}

test('selection resolves direct, connection, and support line identities', () => {
  const fitting = {
    id: 'node-000001',
    name: 'Fitting',
    parentId: null,
    metadata: { lineId: 'L-FIT' },
    content: {},
  } satisfies StudioNode;
  const connection = {
    id: 'node-000002',
    name: 'Segment',
    parentId: null,
    metadata: {},
    content: {
      type: 'connection',
      metadata: { lineId: 'L-CONN', supportLineId: 'L-SEG' },
    },
  } satisfies StudioNode;
  const support = {
    id: 'node-000003',
    name: 'Support',
    parentId: null,
    metadata: { supportLineId: 'L-SEG' },
    content: {},
  } satisfies StudioNode;
  const document = {
    projectId: 'route',
    revision: 2,
    scene: { nodes: [fitting, connection, support] },
    components: [],
  } satisfies StudioDocument;

  assert.equal(routeLineIdForNode(fitting, document), 'L-FIT');
  assert.equal(routeLineIdForNode(connection, document), 'L-CONN');
  assert.equal(routeLineIdForNode(support, document), 'L-CONN');
});

test('a stale route response cannot replace the current selection', async () => {
  const pending = new Map<string, (value: StudioRouteInspection) => void>();
  const store = new RouteInspectionStore({
    routeInspection(lineId) {
      return new Promise((resolve) => pending.set(lineId, resolve));
    },
  });
  const first = store.load('L-A', 2);
  const second = store.load('L-B', 2);
  pending.get('L-B')!(inspection('L-B'));
  await second;
  pending.get('L-A')!(inspection('L-A'));
  await first;
  assert.equal(store.snapshot.lineId, 'L-B');
  assert.equal(store.snapshot.inspection?.lineId, 'L-B');
});

test('route failures leave the store in a retryable error state', async () => {
  const store = new RouteInspectionStore({
    async routeInspection() {
      throw new Error('route unavailable');
    },
  });
  await store.load('L-A', 2);
  assert.equal(store.snapshot.status, 'error');
  assert.match(store.snapshot.error ?? '', /route unavailable/);
});
