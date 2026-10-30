import { spawn } from 'node:child_process';
import { readFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';

export async function launchManagedSidecar({
  applicationRoot,
  workspaceRoot,
  environment = process.env,
}) {
  const root = resolve(applicationRoot);
  const manifest = JSON.parse(
    await readFile(join(root, 'toolchain/manifest.json'), 'utf8'),
  );
  const child = spawn(join(root, manifest.entrypoint), [
    join(root, manifest.packageEntry),
    'sidecar',
    '--workspace',
    resolve(workspaceRoot),
  ], {
    cwd: resolve(workspaceRoot),
    env: {
      PATH: environment.PATH ?? '',
    },
    stdio: ['pipe', 'pipe', 'pipe'],
  });
  return { child, manifest };
}
