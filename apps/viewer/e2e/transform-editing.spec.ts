import { expect, test } from '@playwright/test';

test('edits a primitive draft, reviews the candidate and commits', async ({ page }) => {
  const writes: string[] = [];
  page.on('request', (request) => {
    if (request.method() !== 'POST' || !request.url().endsWith('/rpc')) return;
    const body = request.postDataJSON() as { method?: string };
    if (body.method && (
      body.method.startsWith('changeset.')
      || body.method === 'document.apply'
      || body.method === 'document.undo'
      || body.method === 'document.redo'
    )) {
      writes.push(body.method);
    }
  });
  await page.goto('/');
  await page.getByRole('button', { name: 'Aeration Tank A' }).click();
  await page.getByRole('tab', { name: 'Inspect' }).click();
  writes.length = 0;

  const x = page.getByLabel('Position X');
  const original = Number(await x.inputValue());

  const handle = page.locator('[data-gizmo-axis="x"]').last();
  await expect(handle).toBeVisible();
  const grip = handle.locator('circle');
  let previousPoint: { x: number; y: number } | null = null;
  let stableChecks = 0;
  await expect.poll(async () => {
    const point = await grip.evaluate((element) => {
      const bounds = element.getBoundingClientRect();
      return bounds.width > 0 && bounds.height > 0
        ? { x: bounds.x + bounds.width / 2, y: bounds.y + bounds.height / 2 }
        : null;
    });
    if (
      point
      && previousPoint
      && Math.hypot(point.x - previousPoint.x, point.y - previousPoint.y) < 0.5
    ) {
      stableChecks += 1;
    } else {
      stableChecks = 0;
    }
    previousPoint = point;
    return stableChecks >= 3;
  }, { intervals: [40, 40, 40, 40] }).toBe(true);
  const gripPoint = await grip.evaluate((element) => {
    const bounds = element.getBoundingClientRect();
    return {
      x: bounds.x + bounds.width / 2,
      y: bounds.y + bounds.height / 2,
    };
  });
  await page.mouse.move(gripPoint.x, gripPoint.y);
  await page.mouse.down();
  await page.mouse.move(gripPoint.x + 35, gripPoint.y);
  await page.mouse.up();
  await expect(x).not.toHaveValue(String(original));
  await x.fill(String(original + 5));
  await x.press('Enter');
  await expect(x).toHaveValue(String(original + 5));
  const width = page.getByLabel('Geometry Width');
  const originalWidth = Number(await width.inputValue());
  await width.fill(String(originalWidth + 1));
  await width.press('Enter');
  const material = page.getByLabel('Material');
  const currentMaterial = await material.inputValue();
  const nextMaterial = await material.locator('option').evaluateAll(
    (options, selected) => options
      .map((option) => (option as HTMLOptionElement).value)
      .find((value) => value && value !== selected),
    currentMaterial,
  );
  if (!nextMaterial) throw new Error('No alternative material is available');
  await material.selectOption(nextMaterial);
  expect(writes).toEqual([]);

  await page.getByRole('button', { name: 'Stage draft' }).click();
  await expect(page.getByText('staged', { exact: true })).toBeVisible();
  await page.getByRole('tab', { name: 'Changes' }).click();
  const changes = page.getByRole('region', { name: 'ChangeSet review' });
  await expect(changes).toContainText('primitive.update');
  await expect(changes).toContainText('material.assign');
  await page.getByRole('tab', { name: 'Inspect' }).click();
  await page.getByRole('button', { name: 'Review candidate' }).click();
  await page.getByRole('button', { name: 'Approve & commit' }).click();
  await expect(page.getByRole('status')).toContainText('Revision');
  await expect(page.getByRole('button', { name: 'Stage draft' })).toBeDisabled();
});

test('undo and redo a numeric transform edit', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Aeration Tank B' }).click();
  await page.getByRole('tab', { name: 'Inspect' }).click();
  const input = page.getByLabel('Scale X');
  const original = Number(await input.inputValue());
  await input.fill(String(original + 0.25));
  await input.press('Enter');
  await page.getByRole('button', { name: 'Undo draft' }).click();
  await expect(input).toHaveValue(formatNumber(original));
  await page.getByRole('button', { name: 'Redo draft' }).click();
  await expect(input).toHaveValue(formatNumber(original + 0.25));
});

function formatNumber(value: number): string {
  return String(Number(value.toFixed(3)));
}
