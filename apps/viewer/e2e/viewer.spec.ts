import { expect, test } from '@playwright/test';

test('loads the generic workbench with a persistent scene canvas', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'wastewater-mini' })).toBeVisible();
  await expect(page.getByRole('status')).toContainText('Revision 2');
  await expect(page.locator('canvas[data-layer="scene"]')).toBeVisible();
  await expect(page.getByRole('tab', { name: 'Inspect' })).toBeVisible();
  await expect(page.getByRole('tab', { name: 'Operations' })).toBeVisible();
});

test('stages, reviews, and commits a form change', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('status')).toContainText('Revision 2');
  await page.getByRole('button', { name: 'Aeration Tank A' }).click();
  await page.getByLabel('Name').fill('Aeration Tank Renamed');
  await page.getByRole('button', { name: 'Stage changes' }).click();
  await expect(
    page.getByRole('complementary', { name: 'Review dock' }).getByText('ready'),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Review candidate' }).click();
  await page.getByRole('button', { name: 'Approve & commit' }).click();
  await expect(page.getByRole('status')).toContainText('Revision 3');
  await expect(page.getByRole('button', { name: 'Aeration Tank Renamed' })).toBeVisible();
});

test('keeps review available when Agent Gateway is offline', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('tab', { name: 'Agent' }).click();
  await expect(page.getByText('The Agent Gateway is not connected.')).toBeVisible();
  await expect(page.getByRole('tree').getByRole('button').first()).toBeEnabled();
});

test('loads an arbitrary generic project without industry-specific UI', async ({ page, request }) => {
  const response = await request.post('/rpc', {
    headers: { authorization: 'Bearer studio-e2e' },
    data: {
      jsonrpc: '2.0',
      id: 'generic-project',
      method: 'project.create',
      params: { projectId: 'generic-project' },
    },
  });
  expect(response.ok()).toBe(true);
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'generic-project' })).toBeVisible();
  await expect(page.getByText('Aeration Tank')).toHaveCount(0);
});

test('keeps editing available when Toolchain is incompatible', async ({ page }) => {
  await page.route('**/rpc', async (route) => {
    const request = route.request().postDataJSON() as { method?: string };
    if (request.method !== 'system.capabilities') {
      await route.continue();
      return;
    }
    await route.fulfill({
      contentType: 'application/json',
      body: JSON.stringify({
        result: {
          apiVersion: '1.0',
          schemaVersion: 2,
          methods: [],
          toolchain: {
            state: 'incompatible',
            contractVersion: null,
            packageVersion: null,
            capabilities: [],
            reason: 'required contract 1.x',
          },
        },
      }),
    });
  });
  await page.goto('/');
  await expect(page.getByText('Toolchain incompatible')).toBeVisible();
  await expect(page.locator('canvas[data-layer="scene"]')).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Objects' })).toBeVisible();
});
