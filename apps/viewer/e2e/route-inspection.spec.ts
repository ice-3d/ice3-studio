import { expect, test } from '@playwright/test';

test('inspects a selected fitting route without leaving Inspect', async ({ page }) => {
  await page.route('**/rpc', async (route) => {
    const request = route.request().postDataJSON() as {
      method?: string;
      params?: unknown;
    };
    if (request.method === 'document.get') {
      const response = await route.fetch();
      const body = await response.json();
      body.result.scene.nodes.push({
        id: 'node-fitting',
        name: 'Route reducer',
        parentId: null,
        metadata: {
          role: 'pipe-fitting',
          fittingKind: 'reducer',
          lineId: 'L-E2E',
        },
        content: { type: 'group' },
        transform: {
          translation: { x: 0, y: 0, z: 0 },
          rotation: [0, 0, 0, 1],
          scale: { x: 1, y: 1, z: 1 },
        },
        visible: true,
      });
      body.result.scene.nodes.push({
        id: 'node-host',
        name: 'RAS wall host',
        parentId: null,
        metadata: {},
        content: { type: 'group' },
        transform: {
          translation: { x: 0, y: 0, z: 0 },
          rotation: [0, 0, 0, 1],
          scale: { x: 1, y: 1, z: 1 },
        },
        visible: true,
      });
      await route.fulfill({ response, json: body });
      return;
    }
    if (request.method === 'route.inspect') {
      await route.fulfill({
        json: {
          result: {
            revision: 2,
            lineId: 'L-E2E',
            supportLineIds: ['L-E2E'],
            segments: [{
              nodeId: 'node-segment',
              name: 'Main line',
              medium: 'water',
              diameter: 0.2,
              routePolicy: { kind: 'direct' },
              from: {
                nodeId: 'node-fitting',
                port: 'small',
                medium: 'water',
                diameter: 0.2,
                position: { x: 0, y: 1, z: 0 },
                direction: { x: 1, y: 0, z: 0 },
              },
              to: {
                nodeId: 'node-target',
                port: 'inlet',
                medium: 'water',
                diameter: 0.2,
                position: { x: 10, y: 1, z: 0 },
                direction: { x: -1, y: 0, z: 0 },
              },
              points: [[0, 1, 0], [10, 1, 0]],
            }],
            attachments: [{
              nodeId: 'node-fitting',
              name: 'Route reducer',
              role: 'pipe-fitting',
              fittingKind: 'reducer',
              lineId: 'L-E2E',
              supportLineId: null,
              hostNodeId: 'node-host',
              hostName: 'RAS wall host',
            }],
            supports: [],
            truncated: {
              segments: false,
              points: false,
              attachments: false,
              supports: false,
            },
          },
        },
      });
      return;
    }
    await route.continue();
  });

  await page.goto('/');
  await page.getByPlaceholder('Search objects').fill('Route reducer');
  await page.getByRole('button', { name: 'Route reducer' }).click();
  const inspector = page.getByRole('region', { name: 'Route inspection' });
  await expect(inspector).toContainText('L-E2E');
  await expect(inspector).toContainText('small');
  await expect(inspector).toContainText('0.200 m');
  await page.getByRole('button', { name: 'Select host RAS wall host' }).click();
  await expect(page.getByRole('form', { name: 'Inspect RAS wall host' })).toBeVisible();
});
