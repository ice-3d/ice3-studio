import assert from 'node:assert/strict';
import { rm, readFile } from 'node:fs/promises';
import { spawnSync } from 'node:child_process';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import {
  comparePreparedCase,
  normalizeLegacyReportKind,
} from '../compare-prepare-imports.mjs';

test('prepare baseline pins edges, metrics and budget', async () => {
  const baseline = JSON.parse(await readFile(
    new URL('../../fixtures/parity/prepare-baseline-v1.json', import.meta.url),
    'utf8',
  ));
  assert.equal(baseline.kind, 'ice3-prepare-baseline');
  assert.match(baseline.sourceFiles['prepare-cube.gltf'], /^[a-f0-9]{64}$/);
  assert.match(baseline.sourceFiles['prepare-cube.bin'], /^[a-f0-9]{64}$/);
  assert.equal(baseline.default.edgesPrepared, true);
  assert.equal(baseline.default.stats.triangleCount, 12);
  assert.equal(baseline.default.stats.edgeSegmentCount, 12);
  assert.equal(baseline.default.stats.materialCount, 5);
  assert.equal(baseline.default.edgeRole, 'edges');
  assert.equal(baseline.default.edgeWidthPx, 3);
  assert.deepEqual(baseline.default.edgeColor, [
    0.9019607843137255,
    0.9490196078431372,
    1,
    1,
  ]);
  assert.deepEqual(baseline.default.bounds, {
    min: [-1, -1, -1],
    max: [1, 1, 1],
  });
  assert.match(baseline.default.outputDigest, /^[a-f0-9]{64}$/);
  assert.equal(baseline.default.report.kind, 'ice3-asset-report');
  assert.equal(baseline.default.report.schemaVersion, '1.0');
  assert.deepEqual(baseline.default.nodeBusinessIds, ['prepare-cube/node/0']);
  assert.deepEqual(baseline.default.nodeNames, ['prepared-cube']);
  assert.deepEqual(baseline.default.selectionIds, []);
  assert.equal(baseline.normalizationRules.selectionIdFallback, 'ice3.id');
  assert.equal(baseline.skipEdges.stats.edgeSegmentCount, 0);
  assert.equal(baseline.crease90.stats.edgeSegmentCount, 12);
  assert.equal(baseline.budget.profile, 'interactive-desktop-v1');
  assert.equal(baseline.budget.status, 'pass');
});

test('native Studio build parity report is generated and passes', async () => {
  const reportUrl = new URL('../../output/prepare-parity.json', import.meta.url);
  await rm(reportUrl, { force: true });
  const comparison = spawnSync(
    process.execPath,
    [fileURLToPath(new URL('../../scripts/compare-prepare-imports.mjs', import.meta.url))],
    { encoding: 'utf8' },
  );
  assert.equal(
    comparison.status,
    0,
    `Prepare parity command failed:\n${comparison.stderr}\n${comparison.stdout}`,
  );

  const report = JSON.parse(await readFile(reportUrl, 'utf8'));
  assert.equal(report.kind, 'ice3-prepare-parity');
  assert.equal(report.status, 'pass');
  assert.deepEqual(report.failures, []);
  assert.ok(report.deterministic.byteEqual);
  assert.equal(
    report.deterministic.firstDigest,
    report.deterministic.secondDigest,
  );
  assert.equal(report.comparisons.sourceDigest, true);
  assert.equal(report.comparisons.reportKind, true);
  assert.equal(report.comparisons.reportSchemaVersion, true);
  assert.equal(report.comparisons.reportSourceDigest, true);
  assert.equal(report.comparisons.reportContentDigest, true);
  assert.equal(report.comparisons.contentDigestAgreement, true);
  assert.equal(report.comparisons.profileSourceDigest, true);
  assert.equal(report.comparisons.profileContentDigest, true);
  assert.deepEqual(report.native.stats, report.baseline.stats);
  assert.deepEqual(report.native.bounds, report.baseline.bounds);
  assert.deepEqual(
    report.native.businessNodeIds,
    report.baseline.businessNodeIds,
  );
  assert.deepEqual(report.native.nodeNames, report.baseline.nodeNames);
  assert.deepEqual(report.native.selectionIds, report.baseline.selectionIds);
  assert.deepEqual(report.native.budget, report.baseline.budget);
  assert.equal(report.native.report.kind, report.baseline.report.kind);
  assert.equal(
    report.native.report.schemaVersion,
    report.baseline.report.schemaVersion,
  );
  assert.deepEqual(
    report.native.report.stats,
    report.baseline.report.stats,
  );
  assert.deepEqual(
    report.native.report.budget,
    report.baseline.report.budget,
  );
  assert.equal(report.native.edgeRole, 'edges');
  assert.equal(report.native.edgeWidthPx, 3);
  assert.deepEqual(report.native.edgeColor, report.baseline.edgeColor);
  assert.equal(report.reportKindNormalization.applied, true);
  assert.equal(
    report.reportKindNormalization.legacy,
    'ice3-asset-report',
  );
  assert.equal(
    report.reportKindNormalization.studio,
    'ice3-asset-build-report',
  );
  assert.ok(
    ['studio-cli', 'studio-cli.exe'].includes(report.studioBinary),
    `unexpected Studio executable basename: ${report.studioBinary}`,
  );
  assert.equal(
    report.cases.skipEdges.status,
    'pass',
  );
  assert.equal(
    report.cases.crease90.status,
    'pass',
  );
  assert.ok(report.native.businessNodeIdNormalization.length > 0);
  assert.ok(
    Object.hasOwn(
      report.native.businessNodeIdNormalization[0],
      'original',
    ),
  );
});

test('legacy report normalization never repairs a Studio report regression', () => {
  const expected = {
    sourceDigest: 'source',
    edgesPrepared: true,
    stats: { triangleCount: 12 },
    budget: { profile: 'interactive-desktop-v1', status: 'pass' },
    report: {
      kind: normalizeLegacyReportKind('ice3-asset-report').studio,
      schemaVersion: '1.0',
      stats: { triangleCount: 12 },
      budget: { profile: 'interactive-desktop-v1', status: 'pass' },
    },
    edgeRole: 'edges',
    edgeWidthPx: 3,
    edgeColor: [1, 1, 1, 1],
    edgeMaterialName: 'steel edges',
    businessNodeIds: ['prepare-cube/node/0'],
    nodeNames: ['prepared-cube'],
    selectionIds: ['prepare-cube/node/0'],
    bounds: { min: [-1, -1, -1], max: [1, 1, 1] },
  };
  const native = {
    ...expected,
    profileSourceDigest: 'source',
    profileContentDigest: 'glb',
    report: {
      ...expected.report,
      kind: 'ice3-asset-report',
      sourceDigest: 'source',
      contentDigest: 'glb',
    },
  };
  const comparisons = comparePreparedCase({
    native,
    expected,
    firstDigest: 'glb',
    secondDigest: 'glb',
    binaryDigest: 'binary',
    sourceFileDigest: 'source',
  });

  assert.equal(comparisons.reportKind, false);
  assert.equal(comparisons.reportSchemaVersion, true);
  assert.equal(comparisons.reportContentDigest, false);
  assert.equal(comparisons.profileContentDigest, false);
  assert.equal(comparisons.contentDigestAgreement, true);
});
