import { readFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import {
  bundleDigest,
  pathIsFile,
  sha256File,
} from './toolchain-bundle-lib.mjs';

const args = process.argv.slice(2);
const index = args.indexOf('--bundle');
if (index < 0 || !args[index + 1]) {
  throw new Error('usage: verify-toolchain-bundle --bundle DIR');
}

try {
  const bundle = resolve(args[index + 1]);
  const manifest = JSON.parse(
    await readFile(join(bundle, 'manifest.json'), 'utf8'),
  );
  if (!String(manifest.contractVersion).startsWith('1.')) {
    throw new Error('unsupported Toolchain contract major');
  }
  if (manifest.platform !== normalizePlatform(process.platform)) {
    throw new Error('Toolchain bundle platform mismatch');
  }
  if (manifest.architecture !== process.arch) {
    throw new Error('Toolchain bundle architecture mismatch');
  }
  for (const path of [manifest.entrypoint, manifest.packageEntry]) {
    if (
      typeof path !== 'string'
      || path.startsWith('/')
      || path.includes('..')
    ) {
      throw new Error(`invalid bundle path: ${path}`);
    }
  }
  for (const [path, expected] of Object.entries(manifest.files ?? {})) {
    if (path.startsWith('/') || path.includes('..')) {
      throw new Error(`invalid manifest path: ${path}`);
    }
    const absolute = join(bundle, path);
    if (!(await pathIsFile(absolute))) {
      throw new Error(`missing bundle file: ${path}`);
    }
    const actual = await sha256File(absolute);
    if (actual !== expected) {
      throw new Error(`bundle hash mismatch: ${path}`);
    }
  }
  if (bundleDigest(manifest.files) !== manifest.sha256) {
    throw new Error('bundle aggregate hash mismatch');
  }
  console.log(`Toolchain bundle verified: ${manifest.packageVersion}`);
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
}

function normalizePlatform(platform) {
  if (platform === 'darwin') return 'macos';
  if (platform === 'win32') return 'windows';
  return platform;
}
