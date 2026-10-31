import { existsSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { extname, join, resolve } from 'node:path';
import { expect, test } from '@playwright/test';

const fixtureRoot = resolve(
  process.cwd(),
  '../../../ice3-engine/tests/fixtures/hierarchical-demo',
);
const selectionId = 'hierarchical-demo/node/0';

test('keeps reference selection depth stable and ranges bounded', async ({ page }) => {
  await page.addInitScript(() => {
    const offsets: Array<[number, number]> = [];
    Object.defineProperty(window, '__polygonOffsets', { value: offsets });
    const original = WebGL2RenderingContext.prototype.polygonOffset;
    WebGL2RenderingContext.prototype.polygonOffset = function (
      factor: number,
      units: number,
    ): void {
      offsets.push([factor, units]);
      return original.call(this, factor, units);
    };
  });
  await mockReferenceSession(page);
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'reference-acceptance' })).toBeVisible();
  await expect(page.locator('canvas[data-layer="scene"]')).toBeVisible();
  await expect(page.locator('section.viewport')).toHaveAttribute(
    'data-reference-state',
    'ready',
    { timeout: 10_000 },
  );
  await page.getByRole('button', { name: 'GW-101' }).click();
  await expect(page.getByRole('treeitem')).toHaveAttribute('aria-selected', 'true');
  await expect.poll(async () => {
    const offsets = await page.evaluate(() => (
      window as unknown as { __polygonOffsets: Array<[number, number]> }
    ).__polygonOffsets);
    return offsets.some(([factor, units]) => factor < 0 && units < 0);
  }).toBe(true);
});

async function mockReferenceSession(page: import('@playwright/test').Page): Promise<void> {
  await page.route('**/rpc', async (route) => {
    const request = route.request().postDataJSON() as {
      id: unknown;
      method: string;
    };
    const result = mockedResult(request.method);
    if (result === undefined) {
      await route.continue();
      return;
    }
    await route.fulfill({
      contentType: 'application/json',
      body: JSON.stringify({
        jsonrpc: '2.0',
        id: request.id,
        result,
      }),
    });
  });
  await page.route('**/reference-assets/asset-000001/**', async (route) => {
    const pathname = new URL(route.request().url()).pathname;
    const relative = pathname.includes('/tiled/')
      ? pathname.split('/tiled/')[1]!
      : 'tileset.json';
    const file = join(fixtureRoot, relative);
    if (!existsSync(file)) {
      await route.fulfill({ status: 404 });
      return;
    }
    const isTileset = extname(file) === '.json';
    const body = isTileset
      ? serializeTileset(JSON.parse(await readFile(file, 'utf8')) as TilesetValue)
      : await readFile(file);
    await route.fulfill({
      status: 200,
      contentType: isTileset
        ? 'application/json; charset=utf-8'
        : 'model/gltf-binary',
      body,
    });
  });
}

type TilesetValue = {
  uri?: string;
  [key: string]: unknown;
};

function serializeTileset(value: TilesetValue): Buffer {
  const visit = (candidate: unknown): void => {
    if (Array.isArray(candidate)) {
      for (const child of candidate) visit(child);
      return;
    }
    if (!candidate || typeof candidate !== 'object') return;
    const object = candidate as TilesetValue;
    if (typeof object.uri === 'string' && !object.uri.startsWith('/')) {
      object.uri = `tiled/${object.uri.replace(/^\.\//, '')}`;
    }
    for (const child of Object.values(object)) visit(child);
  };
  visit(value);
  return Buffer.from(JSON.stringify(value));
}

function mockedResult(method: string): unknown {
  if (method === 'document.get') {
    return {
      projectId: 'reference-acceptance',
      revision: 1,
      scene: {
        nodes: [{
          id: 'node-000001',
          name: 'GW-101',
          parentId: null,
          visible: true,
          transform: {
            translation: { x: 0, y: 0, z: 0 },
            rotation: [0, 0, 0, 1],
            scale: { x: 1, y: 1, z: 1 },
          },
          metadata: {},
          content: {
            type: 'reference_instance',
            assetId: 'asset-000001',
            selectionId,
          },
        }],
      },
      components: [],
      materials: [],
    };
  }
  if (method === 'validation.run') return [];
  if (method === 'referenceAsset.get') {
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
      preparedPreviewUrl: null,
      equipmentManifestUrl: '/reference-assets/asset-000001/equipment.json',
    };
  }
  if (method === 'referenceAsset.bindings') {
    return {
      items: [{
        selectionId,
        name: 'GW-101',
      }],
      nextCursor: null,
    };
  }
  return undefined;
}
