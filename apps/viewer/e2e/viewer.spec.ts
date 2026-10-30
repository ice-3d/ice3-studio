import { expect, test } from '@playwright/test';

test('shows current revision and loads the generated preview', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('status')).toContainText('Revision 2');
  await expect(page.getByRole('heading', { name: 'wastewater-mini' })).toBeVisible();
  await expect(page.locator('canvas[data-layer="scene"]')).toBeVisible();
});

test('shows a revision event without unmounting the scene', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('status')).toContainText('Revision 2');
  const canvas = page.locator('canvas[data-layer="scene"]');
  await canvas.evaluate((element) => {
    (element as HTMLCanvasElement).dataset.instanceMarker = 'stable';
  });
  await page.evaluate(() => window.__studioTest.publishRevision(3));
  await expect(page.getByRole('status')).toContainText('Revision 3');
  await expect(canvas).toHaveAttribute('data-instance-marker', 'stable');
});

test('reloads the document and preview for a committed revision', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('status')).toContainText('Revision 2');

  const canvas = page.locator('canvas[data-layer="scene"]');
  await canvas.evaluate((element) => {
    (element as HTMLCanvasElement).dataset.instanceMarker = 'stable';
  });
  const previewLoaded = page.waitForRequest((request) => (
    new URL(request.url()).pathname === '/previews/3'
  ));

  const committed = await page.evaluate(async () => {
    const response = await fetch('/rpc', {
      method: 'POST',
      headers: {
        authorization: 'Bearer studio-e2e',
        'content-type': 'application/json',
      },
      body: JSON.stringify({
        jsonrpc: '2.0',
        id: 'viewer-reload',
        method: 'document.apply',
        params: {
          expectedRevision: 2,
          dryRun: false,
          idempotencyKey: 'viewer-reload',
          operations: [{
            type: 'node.create',
            node_id: 'node-000012',
            parent_id: null,
            name: 'Revision Probe',
            transform: {
              translation: { x: 0, y: 0, z: 0 },
              rotation: [0, 0, 0, 1],
              scale: { x: 1, y: 1, z: 1 },
            },
            content: { type: 'group' },
            material_id: null,
            metadata: { role: 'reviewProbe' },
          }],
        },
      }),
    });
    return response.json();
  });

  expect(committed.result.revision).toBe(3);
  await previewLoaded;
  await expect(page.getByRole('status')).toContainText('Revision 3');
  await expect(page.locator('[data-role="node-list"]')).toContainText('Revision Probe');
  await expect(canvas).toHaveAttribute('data-instance-marker', 'stable');
});
