import { expect, test } from '@playwright/test';

test('loads the generic workbench with a persistent scene canvas', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'wastewater-mini' })).toBeVisible();
  await expect(page.getByRole('status')).toContainText('Revision 2');
  await expect(page.locator('canvas[data-layer="scene"]')).toBeVisible();
  await expect(page.getByRole('tab', { name: 'Inspect' })).toBeVisible();
  await expect(page.getByRole('tab', { name: 'Operations' })).toBeVisible();
});

test('opens on Agent mission control with the current project context', async ({ page }) => {
  await page.goto('/');

  await expect(page.getByRole('tab', { name: 'Agent' })).toHaveAttribute('aria-selected', 'true');
  const mission = page.getByRole('region', { name: 'Agent mission control' });
  await expect(mission).toContainText('wastewater-mini');
  await expect(mission.getByText('2', { exact: true })).toBeVisible();
});

test('stages, reviews, and commits a form change', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('status')).toContainText('Revision 2');
  await page.getByRole('button', { name: 'Secondary Clarifier B' }).click();
  await page.getByRole('button', { name: 'Inspect selected object' }).click();
  await page.getByLabel('Name').fill('Secondary Clarifier Renamed');
  await page.getByRole('button', { name: 'Stage changes' }).click();
  await expect(
    page.getByRole('complementary', { name: 'Review dock' }).getByText('ready'),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Review candidate' }).click();
  await page.getByRole('button', { name: 'Approve & commit' }).click();
  await expect(page.getByRole('status')).toContainText('Revision 3');
  await expect(page.getByRole('button', { name: 'Secondary Clarifier Renamed' })).toBeVisible();
});

test('keeps review available when Agent Gateway is offline', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('tab', { name: 'Agent' }).click();
  await expect(page.getByText('Agent Gateway is offline')).toBeVisible();
  await expect(page.getByRole('tree').getByRole('button').first()).toBeEnabled();
  await page.getByRole('button', { name: 'Open manual controls' }).click();
  await expect(page.getByRole('tab', { name: 'Inspect' })).toHaveAttribute('aria-selected', 'true');
});

test('filters the object tree and keeps entity selection available', async ({ page }) => {
  await page.goto('/');
  const search = page.getByPlaceholder('Search objects');
  await expect(search).toBeVisible();
  await search.fill('Aeration Tank A');

  await expect(page.getByRole('button', { name: 'Aeration Tank A' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Secondary Clarifier A' })).toHaveCount(0);

  await page.getByRole('button', { name: 'Aeration Tank A' }).click();
  await expect(
    page.getByRole('region', { name: 'Agent mission control' }),
  ).toContainText('Aeration Tank A');
  await page.getByRole('button', { name: 'Inspect selected object' }).click();
  await expect(page.getByLabel('Inspect Aeration Tank A')).toBeVisible();
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

test('uses the native asset pipeline without a driver status surface', async ({ page }) => {
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
          compute: {
            mode: 'cpu',
            selectedBackend: 'cpu',
            adapter: 'reference',
            tier: 'cpu',
            memoryBudgetBytes: 0,
            health: 'ready',
            fallbackReason: null,
            queueDepth: 0,
            maxConcurrentJobs: 1,
            gpuThresholdTriangles: 0,
            lastKernelMs: 0,
            lastUploadBytes: 0,
            lastReadbackBytes: 0,
            lastCacheHits: 0,
          },
        },
      }),
    });
  });
  await page.goto('/');
  await expect(page.getByRole('region', { name: 'Agent mission control' })).toBeVisible();
  await page.getByRole('tab', { name: 'Inspect' }).click();
  await expect(page.locator('canvas[data-layer="scene"]')).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Objects' })).toBeVisible();
  await expect(page.getByText(/Toolchain/i)).toHaveCount(0);
  await expect(page.getByLabel('Toolchain status')).toHaveCount(0);
});
