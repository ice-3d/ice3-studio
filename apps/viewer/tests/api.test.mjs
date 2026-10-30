import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

test('viewer exposes the required API operations', async () => {
  const source = await readFile(new URL('../src/api.ts', import.meta.url), 'utf8');
  for (const operation of [
    'document.get',
    'validation.run',
    'export.runtimePreview',
  ]) {
    assert.match(source, new RegExp(operation.replace('.', '\\.')));
  }
  assert.match(source, /diagnosticCode/);
  assert.match(source, /preview\.revision/);
});
