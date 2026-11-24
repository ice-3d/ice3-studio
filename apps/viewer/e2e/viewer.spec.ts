import { expect, test } from '@playwright/test';

test('loads the generic workbench with a persistent scene canvas', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'wastewater-mini' })).toBeVisible();
  await expect(page.getByRole('status')).toContainText(/Revision \d+/);
  await expect(page.locator('canvas[data-layer="scene"]')).toBeVisible();
  await expect(page.getByRole('tab', { name: 'Inspect' })).toHaveAttribute(
    'aria-selected',
    'true',
  );
  await expect(page.getByRole('tab', { name: 'Agent' })).toHaveCount(0);
  await expect(page.getByRole('tab', { name: 'Operations' })).toBeVisible();
});

test('opens on the human Inspect workflow', async ({ page }) => {
  await page.goto('/');

  await expect(page.getByRole('tab', { name: 'Inspect' })).toHaveAttribute(
    'aria-selected',
    'true',
  );
  await expect(page.getByText('No object selected')).toBeVisible();
  await expect(page.getByRole('tab', { name: 'Changes' })).toBeVisible();
});

test('keeps Scene, Objects and Review ordered on mobile without overflow', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/');

  const scene = page.getByRole('region', { name: 'Model viewport' });
  const objects = page.getByRole('navigation', { name: 'Project navigation' });
  const review = page.getByRole('complementary', { name: 'Review dock' });
  const inspect = page.getByRole('tab', { name: 'Inspect' });
  await expect(review).toBeVisible();
  await expect(inspect).toBeVisible();

  const [sceneBox, objectsBox, reviewBox, inspectBox] = await Promise.all([
    scene.boundingBox(),
    objects.boundingBox(),
    review.boundingBox(),
    inspect.boundingBox(),
  ]);
  if (!sceneBox || !objectsBox || !reviewBox || !inspectBox) {
    throw new Error('Mobile workbench regions must have layout boxes');
  }
  expect(sceneBox.y).toBeLessThan(objectsBox.y);
  expect(objectsBox.y).toBeLessThan(reviewBox.y);
  expect(inspectBox.height).toBeGreaterThanOrEqual(44);
  expect(await page.evaluate(
    () => document.documentElement.scrollWidth <= document.documentElement.clientWidth,
  )).toBe(true);
});

test('stages, reviews, and commits a form change', async ({ page }) => {
  await page.goto('/');
  const initialRevision = await currentRevision(page);
  await page.getByRole('button', { name: 'Secondary Clarifier B' }).click();
  await expect(page.getByLabel('Position X')).toBeVisible();
  await expect(page.getByLabel('Rotation Y')).toBeVisible();
  await expect(page.getByLabel('Scale Z')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Stage draft' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Move mode' })).toBeVisible();
  await page.getByLabel('Name').fill('Secondary Clarifier Renamed');
  await page.getByRole('button', { name: 'Stage draft' }).click();
  await expect(
    page
      .getByRole('complementary', { name: 'Review dock' })
      .locator('.review-actions__status')
      .getByText('ready'),
  ).toBeVisible();
  await page.getByRole('tab', { name: 'Changes' }).click();
  const changes = page.getByRole('region', { name: 'ChangeSet review' });
  await expect(changes).toContainText('Workbench form edits');
  await expect(changes).toContainText('node.update');
  await expect(changes).toContainText('Secondary Clarifier B');
  await expect(changes).toContainText('1 updated object');
  await page.getByRole('tab', { name: 'Operations' }).click();
  await expect(page.getByRole('region', { name: 'Evidence drawer' })).toContainText(
    'Secondary Clarifier B',
  );
  await page.getByRole('tab', { name: 'Inspect' }).click();
  await page.getByRole('button', { name: 'Review candidate' }).click();
  await page.getByRole('button', { name: 'Approve & commit' }).click();
  await expect(page.getByRole('status')).toContainText(`Revision ${initialRevision + 1}`);
  await expect(page.getByRole('button', { name: 'Secondary Clarifier Renamed' })).toBeVisible();
});

test('removes a staged operation and invalidates the candidate', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Secondary Clarifier A' }).click();
  await page.getByLabel('Name').fill('Secondary Clarifier Temporary');
  await page.getByRole('button', { name: 'Stage draft' }).click();
  await page.getByRole('tab', { name: 'Changes' }).click();

  const changes = page.getByRole('region', { name: 'ChangeSet review' });
  await expect(changes).toContainText('Secondary Clarifier Temporary');
  await page.getByRole('button', {
    name: 'Remove node.update for Secondary Clarifier A',
  }).click();

  await expect(changes).toContainText('No staged operations');
  await expect(changes).toContainText('blocked');
  await page.getByRole('tab', { name: 'Inspect' }).click();
  await expect(page.getByRole('button', { name: 'Approve & commit' })).toBeDisabled();
});

test('filters the object tree and keeps entity selection available', async ({ page }) => {
  await page.goto('/');
  const search = page.getByPlaceholder('Search objects');
  await expect(search).toBeVisible();
  await search.fill('Aeration Tank A');

  await expect(page.getByRole('button', { name: 'Aeration Tank A' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Secondary Clarifier A' })).toHaveCount(0);

  await page.getByRole('button', { name: 'Aeration Tank A' }).click();
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
  await expect(page.getByRole('tab', { name: 'Inspect' })).toHaveAttribute(
    'aria-selected',
    'true',
  );
  await expect(page.locator('canvas[data-layer="scene"]')).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Objects' })).toBeVisible();
  await expect(page.getByText(/Toolchain/i)).toHaveCount(0);
  await expect(page.getByLabel('Toolchain status')).toHaveCount(0);
});

async function currentRevision(page: import('@playwright/test').Page): Promise<number> {
  const text = await page.getByRole('status').textContent();
  const match = text?.match(/Revision (\d+)/);
  if (!match) throw new Error(`Revision status is invalid: ${text}`);
  return Number(match[1]);
}
