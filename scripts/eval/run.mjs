// Agent evaluation runner.
//
// Every task runs `attempts` times against a fresh server and a fresh copy of
// the wastewater fixture. A task counts as passed only when every attempt
// passes, so flaky behaviour shows up as a lower pass^N instead of hiding
// behind a lucky run.
//
// Usage:
//   node scripts/eval/run.mjs [--attempts 3] [--json target/eval-report.json]

import { spawn, spawnSync } from 'node:child_process';
import { cp, mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';

import { tasks } from './tasks.mjs';

const repositoryRoot = fileURLToPath(new URL('../..', import.meta.url));
const fixture = join(repositoryRoot, 'fixtures', 'wastewater-mini');
const binary = join(repositoryRoot, 'target', 'debug', 'studio-cli');

function argument(name, fallback) {
  const index = process.argv.indexOf(`--${name}`);
  return index >= 0 ? process.argv[index + 1] : fallback;
}

const attempts = Number(argument('attempts', '3'));
const reportPath = argument('json', join(repositoryRoot, 'target', 'eval-report.json'));

class RpcClient {
  constructor(baseUrl, { token, cookie } = {}) {
    this.baseUrl = baseUrl;
    this.token = token;
    this.cookie = cookie;
  }

  async call(method, params) {
    const headers = { 'content-type': 'application/json' };
    if (this.token) headers.authorization = `Bearer ${this.token}`;
    if (this.cookie) headers.cookie = this.cookie;
    const response = await fetch(`${this.baseUrl}/rpc`, {
      method: 'POST',
      headers,
      body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }),
    });
    const body = await response.json();
    if (body.error) {
      const error = new Error(body.error.message);
      error.code = body.error.diagnosticCode ?? body.error.code;
      throw error;
    }
    return body.result;
  }
}

/// The approval cookie is a one-shot 303 bootstrap. Redirects must not be
/// followed, or fetch swallows the Set-Cookie header.
async function approvalCookie(bootstrapUrl) {
  const response = await fetch(bootstrapUrl, { redirect: 'manual' });
  const header = response.headers.getSetCookie?.()[0] ?? response.headers.get('set-cookie');
  if (!header) {
    throw new Error(`bootstrap returned ${response.status} without an approval cookie`);
  }
  return header.split(';')[0];
}

function startServer(workspace) {
  const child = spawn(
    binary,
    [
      'serve',
      '--workspace', workspace,
      '--project', 'eval',
      '--port', '0',
      '--fixture', join(workspace, 'seed'),
    ],
    { cwd: repositoryRoot, stdio: ['ignore', 'pipe', 'pipe'] },
  );

  const lines = [];
  const errors = [];
  child.stderr.on('data', (chunk) => errors.push(chunk.toString()));

  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      child.kill();
      reject(new Error(`server did not start within 20s: ${errors.join('')}`));
    }, 20_000);

    createInterface({ input: child.stdout }).on('line', (line) => {
      const value = line.trim();
      if (!value) return;
      lines.push(value);
      if (lines.length < 3) return;
      clearTimeout(timer);
      const [address, token, bootstrapUrl] = lines;
      const baseUrl = `http://${address}`;
      resolve({ child, baseUrl, token, bootstrapUrl });
    });

    child.on('exit', (code) => {
      clearTimeout(timer);
      if (lines.length < 3) {
        reject(new Error(`server exited with ${code} before reporting: ${errors.join('')}`));
      }
    });
  });
}

async function runAttempt(task) {
  const workspace = await mkdtemp(join(tmpdir(), 'studio-eval-'));
  await cp(fixture, join(workspace, 'seed'), { recursive: true });
  let server;
  try {
    server = await startServer(workspace);
    const client = new RpcClient(server.baseUrl, { token: server.token });
    // Only tasks that need human authority pay for the bootstrap round-trip.
    let approver;
    const approvalClient = async () => {
      approver ??= new RpcClient(server.baseUrl, {
        cookie: await approvalCookie(server.bootstrapUrl),
      });
      return approver;
    };

    const assert = (condition, message) => {
      if (!condition) throw new Error(message);
    };

    const result = await task.run({ client, approvalClient });
    task.check(result, { assert });
    return { passed: true };
  } catch (error) {
    return { passed: false, error: error.message };
  } finally {
    if (server) {
      server.child.kill();
      await new Promise((resolve) => server.child.once('exit', resolve));
    }
    await rm(workspace, { recursive: true, force: true });
  }
}

async function main() {
  const build = spawnSync('cargo', ['build', '-q', '-p', 'studio-cli'], {
    cwd: repositoryRoot,
    stdio: 'inherit',
  });
  if (build.status !== 0) throw new Error('cargo build failed');

  const started = Date.now();
  const report = [];

  for (const [index, task] of tasks.entries()) {
    const results = [];
    for (let attempt = 0; attempt < attempts; attempt += 1) {
      results.push(await runAttempt(task));
    }
    const stable = results.every((result) => result.passed);
    report.push({ id: task.id, goal: task.goal, stable, attempts: results });
    const mark = stable ? 'PASS' : 'FAIL';
    const detail = stable ? '' : ` — ${results.find((r) => !r.passed)?.error ?? ''}`;
    console.log(`[${index + 1}/${tasks.length}] ${mark} ${task.id}${detail}`);
  }

  const stable = report.filter((entry) => entry.stable).length;
  const summary = {
    tasks: report.length,
    attemptsPerTask: attempts,
    stable,
    unstable: report.filter((entry) => !entry.stable).map((entry) => entry.id),
    durationMs: Date.now() - started,
  };

  await mkdir(dirname(reportPath), { recursive: true });
  await writeFile(
    reportPath,
    `${JSON.stringify({ summary, tasks: report }, null, 2)}\n`,
    'utf8',
  );

  console.log('');
  console.log(
    `pass^${attempts}: ${stable}/${report.length} stable · ${(summary.durationMs / 1000).toFixed(1)}s`,
  );
  console.log(`report: ${reportPath}`);

  if (stable !== report.length) process.exitCode = 1;
}

await main();
