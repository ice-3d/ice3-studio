import { expect, test } from '@playwright/test';

test('runs an Agent objective and hands off a ChangeSet for review', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByLabel('Run objective')).toBeVisible();
  await page.getByLabel('Run objective').fill('Rename the clarifier for review');
  await page.getByRole('button', { name: 'Run with Agent' }).click();

  await expect(page.getByText('Planning the edit')).toBeVisible();
  await expect(
    page.getByRole('region', { name: 'Agent tool activity' })
      .getByText('scene.search')
      .first(),
  ).toBeVisible();
  await expect(page.getByText('Found the clarifier').first()).toBeVisible();

  const changes = page.getByRole('region', { name: 'ChangeSet review' });
  await expect(changes).toBeVisible();
  await expect(changes).toContainText('Agent E2E');

  await page.getByRole('tab', { name: 'Inspect' }).click();
  await expect(page.getByRole('button', { name: 'Approve & commit' })).toBeDisabled();
  await page.getByRole('button', { name: 'Review candidate' }).click();
  await page.getByRole('button', { name: 'Approve & commit' }).click();
  await expect(page.getByRole('status')).toContainText(/Revision \d+/);
});

test('Gateway disconnect leaves the human path available', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByLabel('Run objective')).toBeVisible();
  await page.getByLabel('Run objective').fill('[disconnect]');
  await page.getByRole('button', { name: 'Run with Agent' }).click();
  await expect(page.getByText('Agent Gateway is offline')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Open manual controls' })).toBeEnabled();
  await expect(page.getByRole('tree').getByRole('button').first()).toBeEnabled();
});
