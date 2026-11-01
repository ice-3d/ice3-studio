import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import test from 'node:test';
import { buildPlan as buildLegacyPlan } from '../../scripts/build-wastewater-model.mjs';

const digest = (value) => createHash('sha256')
  .update(JSON.stringify(value))
  .digest('hex');

test('module split preserves the legacy production plan', async () => {
  const module = await import('../../scripts/wastewater-plant/site-plan.mjs');
  assert.equal(
    digest(module.buildPlan('wastewater-plant')),
    digest(buildLegacyPlan('wastewater-plant')),
  );
  assert.equal(
    digest(buildLegacyPlan('wastewater-plant')),
    '61409900f541447c8cd415d10067e8c17f246d660c9385b587bd4a4ef380b7ce',
  );
});
