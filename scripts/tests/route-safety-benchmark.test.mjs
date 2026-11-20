import assert from 'node:assert/strict';
import test from 'node:test';
import {
  summarizeRouteSafetyRuns,
} from '../benchmark-route-safety.mjs';

test('route safety report enforces latency and memory limits', () => {
  const report = summarizeRouteSafetyRuns([
    { durationMs: 80, peakBytes: 20 * 1024 * 1024 },
    { durationMs: 100, peakBytes: 24 * 1024 * 1024 },
  ]);
  assert.equal(report.p95Ms, 100);
  assert.equal(report.withinLimits, true);
  assert.throws(
    () => summarizeRouteSafetyRuns([
      { durationMs: 260, peakBytes: 70 * 1024 * 1024 },
    ]),
    /route safety performance gate failed/,
  );
});
