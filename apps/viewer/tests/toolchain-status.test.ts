import assert from 'node:assert/strict';
import test from 'node:test';
import { formatToolchainStatus } from '../src/toolchain/ToolchainPanel';

test('formats Toolchain state separately from rendering and studio compute', () => {
  const view = formatToolchainStatus({
    state: 'ready',
    contractVersion: '1.0.0',
    packageVersion: '0.2.0',
    capabilities: ['asset.preview', 'asset.partition'],
    reason: null,
  });
  assert.equal(view.label, 'Toolchain ready');
  assert.equal(view.version, 'contract 1.0.0 · package 0.2.0');
  assert.equal(view.reason, null);
});

test('labels incompatible and unavailable states', () => {
  assert.equal(formatToolchainStatus({
    state: 'incompatible',
    contractVersion: null,
    packageVersion: null,
    capabilities: [],
    reason: 'required 1.x',
  }).label, 'Toolchain incompatible');
  assert.equal(formatToolchainStatus(null).label, 'Toolchain starting');
});
