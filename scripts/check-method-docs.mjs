// Keep the method tables in the reference docs honest.
//
// `system.capabilities` is the authoritative list: it is what a client actually
// receives at runtime. Every method it reports must appear in both reference
// tables, otherwise the docs quietly drift — which they did for scene.bom,
// scene.clashes and scene.measure.

import { spawn } from 'node:child_process';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createInterface } from 'node:readline';

const binary = join('target', 'debug', process.platform === 'win32' ? 'studio-cli.exe' : 'studio-cli');
const documents = ['docs/reference/api.md', 'docs/reference/capabilities.md'];

/// Rows like `| \`scene.search\` |` are method rows; file names such as
/// `project.json` also match the shape, so they are excluded explicitly.
const notMethods = new Set(['project.json']);

function documentedMethods(text) {
  const found = new Set();
  for (const match of text.matchAll(/^\| `([a-zA-Z]+\.[a-zA-Z]+)`/gm)) {
    if (!notMethods.has(match[1])) found.add(match[1]);
  }
  return found;
}

function liveMethods() {
  const workspace = mkdtemp(join(tmpdir(), 'studio-docs-')).catch(() => null);
  return workspace.then((root) => {
    if (!root) throw new Error('could not create a temporary workspace');
    const child = spawn(binary, [
      'serve', '--workspace', root, '--project', 'docs', '--port', '0',
    ], { stdio: ['ignore', 'pipe', 'inherit'] });

    return new Promise((resolve, reject) => {
      const lines = [];
      const timer = setTimeout(() => {
        child.kill();
        reject(new Error('server did not start within 20s'));
      }, 20_000);

      createInterface({ input: child.stdout }).on('line', async (line) => {
        if (!line.trim()) return;
        lines.push(line.trim());
        if (lines.length < 3) return;
        clearTimeout(timer);
        const [address, token] = lines;
        try {
          const response = await fetch(`http://${address}/rpc`, {
            method: 'POST',
            headers: { 'content-type': 'application/json', authorization: `Bearer ${token}` },
            body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'system.capabilities', params: {} }),
          });
          const body = await response.json();
          if (!body.result?.methods) throw new Error(`capabilities failed: ${JSON.stringify(body)}`);
          resolve({
            methods: body.result.methods.filter((entry) => entry.requires_auth !== undefined || typeof entry === 'string'),
            names: body.result.methods.map((entry) => (typeof entry === 'string' ? entry : entry.name)),
            root,
          });
        } catch (error) {
          reject(error);
        } finally {
          child.kill();
        }
      });

      child.on('exit', (code) => {
        clearTimeout(timer);
        if (lines.length < 3) reject(new Error(`server exited with ${code} before reporting`));
      });
    });
  });
}

const { names, root } = await liveMethods();
await rm(root, { recursive: true, force: true });

let failed = false;
for (const path of documents) {
  const documented = documentedMethods(await readFile(path, 'utf8'));
  const missing = names.filter((name) => !documented.has(name));
  const extra = [...documented].filter((name) => !names.includes(name));
  if (missing.length > 0) {
    failed = true;
    console.error(`${path}: missing ${missing.length} method(s): ${missing.join(', ')}`);
  }
  if (extra.length > 0) {
    failed = true;
    console.error(`${path}: documents ${extra.length} method(s) that do not exist: ${extra.join(', ')}`);
  }
  if (missing.length === 0 && extra.length === 0) {
    console.log(`${path}: ${documented.size} methods, complete`);
  }
}

if (failed) {
  console.error(`\nRun against ${names.length} live methods from system.capabilities.`);
  process.exit(1);
}
