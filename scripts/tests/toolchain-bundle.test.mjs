import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import {
  chmod,
  mkdtemp,
  readFile,
  rm,
  writeFile,
  mkdir,
} from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import test from 'node:test';

async function createInputs(root) {
  const packageRoot = join(root, 'fake-toolchain');
  const packageDist = join(packageRoot, 'dist');
  await mkdir(packageDist, { recursive: true });
  await writeFile(
    join(packageRoot, 'package.json'),
    JSON.stringify({ name: '@ice3/toolchain', version: '0.2.0' }),
  );
  await writeFile(join(packageDist, 'main.js'), 'console.log("sidecar");\n');
  const node = join(root, 'fake-node');
  await writeFile(node, '#!/bin/sh\nif [ "$1" = "--version" ]; then echo v24.22.1; fi\n');
  await chmod(node, 0o755);
  return { packageRoot, node, bundle: join(root, 'bundle') };
}

test('stages and verifies a relocatable sidecar bundle', async () => {
  const root = await mkdtemp(join(tmpdir(), 'ice3-toolchain-bundle-'));
  try {
    const inputs = await createInputs(root);
    const staged = spawnSync(process.execPath, [
      'scripts/stage-toolchain.mjs',
      '--package',
      inputs.packageRoot,
      '--node',
      inputs.node,
      '--output',
      inputs.bundle,
    ], { cwd: resolve('.'), encoding: 'utf8' });
    assert.equal(staged.status, 0, staged.stderr);

    const manifest = JSON.parse(
      await readFile(join(inputs.bundle, 'manifest.json'), 'utf8'),
    );
    assert.equal(manifest.contractVersion, '1.0.0');
    assert.equal(manifest.packageVersion, '0.2.0');
    assert.match(manifest.sha256, /^[a-f0-9]{64}$/);
    assert.equal(manifest.entrypoint, 'runtime/node');
    assert.equal(manifest.packageEntry, 'toolchain/dist/main.js');

    const verified = spawnSync(process.execPath, [
      'scripts/verify-toolchain-bundle.mjs',
      '--bundle',
      inputs.bundle,
    ], { cwd: resolve('.'), encoding: 'utf8' });
    assert.equal(verified.status, 0, verified.stderr);

    await writeFile(
      join(inputs.bundle, 'toolchain/dist/main.js'),
      'corrupted\n',
    );
    const corrupted = spawnSync(process.execPath, [
      'scripts/verify-toolchain-bundle.mjs',
      '--bundle',
      inputs.bundle,
    ], { cwd: resolve('.'), encoding: 'utf8' });
    assert.notEqual(corrupted.status, 0);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('rejects platform or entrypoint mismatches', async () => {
  const root = await mkdtemp(join(tmpdir(), 'ice3-toolchain-bundle-'));
  try {
    const inputs = await createInputs(root);
    const staged = spawnSync(process.execPath, [
      'scripts/stage-toolchain.mjs',
      '--package',
      inputs.packageRoot,
      '--node',
      inputs.node,
      '--output',
      inputs.bundle,
    ], { cwd: resolve('.'), encoding: 'utf8' });
    assert.equal(staged.status, 0, staged.stderr);
    const manifestPath = join(inputs.bundle, 'manifest.json');
    const original = JSON.parse(await readFile(manifestPath, 'utf8'));

    await writeFile(manifestPath, JSON.stringify({
      ...original,
      platform: 'unsupported',
    }));
    const platform = spawnSync(process.execPath, [
      'scripts/verify-toolchain-bundle.mjs',
      '--bundle',
      inputs.bundle,
    ], { cwd: resolve('.'), encoding: 'utf8' });
    assert.notEqual(platform.status, 0);

    await writeFile(manifestPath, JSON.stringify({
      ...original,
      packageEntry: '../outside.js',
    }));
    const entrypoint = spawnSync(process.execPath, [
      'scripts/verify-toolchain-bundle.mjs',
      '--bundle',
      inputs.bundle,
    ], { cwd: resolve('.'), encoding: 'utf8' });
    assert.notEqual(entrypoint.status, 0);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
