import { expect, test } from '@playwright/test';

test('explode separates the scene', async ({ page }) => {
  await page.goto('/');
  const canvas = page.locator('canvas[data-layer="scene"]');
  await expect(canvas).toBeVisible();
  await page.waitForTimeout(1200);
  const before = await canvas.screenshot();

  await page.getByRole('button', { name: 'Explode', exact: true }).click();
  await page.waitForTimeout(600);
  const after = await canvas.screenshot();

  expect(after.equals(before)).toBe(false);
});
