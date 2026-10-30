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
