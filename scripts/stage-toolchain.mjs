import { execFileSync } from 'node:child_process';
import {
  chmod,
  cp,
  mkdir,
  readFile,
  rename,
  rm,
  stat,
  writeFile,
} from 'node:fs/promises';
import { dirname, join, relative, resolve } from 'node:path';
import {
  bundleDigest,
  listFiles,
  pathIsFile,
  sha256File,
} from './toolchain-bundle-lib.mjs';

const args = process.argv.slice(2);
const value = (name) => {
  const index = args.indexOf(name);
  return index < 0 ? undefined : args[index + 1];
};
const packageRoot = value('--package');
const nodePath = value('--node');
const output = value('--output');
if (!packageRoot || !nodePath || !output) {
  throw new Error('usage: stage-toolchain --package DIR --node FILE --output DIR');
}

const sourcePackage = resolve(packageRoot);
const sourceNode = resolve(nodePath);
const destination = resolve(output);
if (!(await pathIsFile(join(sourcePackage, 'package.json')))) {
  throw new Error('Toolchain package.json is missing');
}
if (!(await pathIsFile(join(sourcePackage, 'dist/main.js')))) {
  throw new Error('Toolchain sidecar entry is missing');
}
if (!(await pathIsFile(sourceNode))) {
  throw new Error('Node.js executable is missing');
}
const packageJson = JSON.parse(
  await readFile(join(sourcePackage, 'package.json'), 'utf8'),
);
const template = JSON.parse(
  await readFile('toolchain/manifest.template.json', 'utf8'),
);
const nodeVersion = execFileSync(sourceNode, ['--version'], {
  encoding: 'utf8',
}).trim();
const temporary = `${destination}.${process.pid}.tmp`;
await rm(temporary, { recursive: true, force: true });
await mkdir(temporary, { recursive: true });
try {
  await cp(sourcePackage, join(temporary, 'toolchain'), {
    recursive: true,
    filter: (source) => !relative(sourcePackage, source).startsWith('.git'),
  });
  const dependencies = await findNodeModules(sourcePackage);
  if (dependencies) {
    await cp(
      dependencies,
      join(temporary, 'toolchain/node_modules'),
      { recursive: true },
    );
  }
  await mkdir(join(temporary, 'runtime'), { recursive: true });
  await cp(sourceNode, join(temporary, 'runtime/node'));
  await chmod(join(temporary, 'runtime/node'), 0o755);
  const files = Object.fromEntries(await Promise.all(
    (await listFiles(temporary)).map(async (path) => [
      path,
      await sha256File(join(temporary, path)),
    ]),
  ));
  const manifest = {
    schemaVersion: 1,
    contractVersion: template.contractVersion,
    packageVersion: packageJson.version,
    nodeVersion,
    platform: normalizePlatform(process.platform),
    architecture: process.arch,
    entrypoint: 'runtime/node',
    packageEntry: 'toolchain/dist/main.js',
    sha256: bundleDigest(files),
    files,
  };
  await writeFile(
    join(temporary, 'manifest.json'),
    `${JSON.stringify(manifest, null, 2)}\n`,
  );
  await mkdir(dirname(destination), { recursive: true });
  await rm(destination, { recursive: true, force: true });
  await rename(temporary, destination);
} catch (error) {
  await rm(temporary, { recursive: true, force: true });
  throw error;
}

function normalizePlatform(platform) {
  if (platform === 'darwin') return 'macos';
  if (platform === 'win32') return 'windows';
  return platform;
}

async function findNodeModules(start) {
  let current = resolve(start);
  while (true) {
    const candidate = join(current, 'node_modules');
    try {
      if ((await stat(candidate)).isDirectory()) return candidate;
    } catch {
      // Continue toward the filesystem root.
    }
    const parent = resolve(current, '..');
    if (parent === current) return undefined;
    current = parent;
  }
}
