import { execFileSync, spawn } from 'node:child_process';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createInterface } from 'node:readline';
import { pathToFileURL } from 'node:url';

const MAX_WASTEWATER_P95_MS = 150;
const MAX_LARGE_FIXTURE_P95_MS = 250;
const MAX_EXTRA_BYTES = 64 * 1024 * 1024;

export function summarizeRouteSafetyRuns(
  runs,
  {
    maxP95Ms = MAX_WASTEWATER_P95_MS,
    maxPeakBytes = MAX_EXTRA_BYTES,
  } = {},
) {
  if (!Array.isArray(runs) || runs.length === 0) {
    throw new Error('route safety performance gate failed: no benchmark runs');
  }
  const durations = runs.map(({ durationMs }) => durationMs).sort((a, b) => a - b);
  const peakBytes = Math.max(...runs.map(({ peakBytes }) => peakBytes));
  const percentile = (value) => {
    const index = Math.max(0, Math.ceil(value * durations.length) - 1);
    return durations[index];
  };
  const p50Ms = percentile(0.5);
  const p95Ms = percentile(0.95);
  const withinLimits = p95Ms <= maxP95Ms && peakBytes <= maxPeakBytes;
  if (!withinLimits) {
    throw new Error(
      `route safety performance gate failed: p95=${p95Ms.toFixed(2)}ms, `
      + `peak=${(peakBytes / 1024 / 1024).toFixed(2)}MiB`,
    );
  }
  return { p50Ms, p95Ms, peakBytes, withinLimits };
}

async function main() {
  const root = resolve('.');
  await run('cargo', ['build', '--release', '-p', 'studio-cli']);
  const binary = resolve('target/release/studio-cli');
  const workspace = await mkdtemp(join(tmpdir(), 'ice3-route-benchmark-'));
  let server;
  try {
    await run('node', [
      'scripts/build-wastewater-model.mjs',
      '--workspace',
      workspace,
    ]);
    const project = join(workspace, 'wastewater-plant');
    const counters = JSON.parse(execFileSync(
      binary,
      ['route-safety-report', '--project', project],
      { encoding: 'utf8' },
    ));
    server = spawn(
      binary,
      [
        'serve',
        '--workspace',
        workspace,
        '--project',
        'wastewater-plant',
        '--port',
        '0',
        '--write-mode',
        'review-required',
      ],
      {
        cwd: root,
        env: {
          ...process.env,
          STUDIO_SESSION_TOKEN: 'route-benchmark',
          STUDIO_VIEWER_DIR: resolve('apps/viewer/dist'),
        },
        stdio: ['ignore', 'pipe', 'inherit'],
      },
    );
    const { address, token } = await waitForServer(server);
    const baselineRss = processRss(server.pid);
    let peakRss = baselineRss;
    const call = async () => {
      const started = performance.now();
      const response = await fetch(`http://${address}/rpc`, {
        method: 'POST',
        headers: {
          authorization: `Bearer ${token}`,
          'content-type': 'application/json',
        },
        body: JSON.stringify({
          jsonrpc: '2.0',
          id: 'route-safety',
          method: 'validation.run',
          params: {},
        }),
      });
      const body = await response.json();
      if (body.error) throw new Error(body.error.message);
      return performance.now() - started;
    };
    for (let index = 0; index < 5; index += 1) await call();
    const runs = [];
    for (let index = 0; index < 20; index += 1) {
      const durationMs = await call();
      peakRss = Math.max(peakRss, processRss(server.pid));
      runs.push({
        durationMs,
        peakBytes: Math.max(0, (peakRss - baselineRss) * 1024),
      });
    }
    const summary = summarizeRouteSafetyRuns(runs);
    const report = {
      kind: 'ice3-route-safety-benchmark',
      schemaVersion: '1.0',
      projectId: 'wastewater-plant',
      revision: counters.revision,
      stats: counters.stats,
      diagnostics: counters.diagnostics.length,
      ...summary,
      maxP95Ms: MAX_WASTEWATER_P95_MS,
      maxPeakBytes: MAX_EXTRA_BYTES,
    };
    await mkdir('output', { recursive: true });
    await writeFile(
      'output/route-safety-benchmark.json',
      `${JSON.stringify(report, null, 2)}\n`,
    );
    process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  } finally {
    if (server) {
      server.kill('SIGTERM');
      await new Promise((resolveExit) => server.once('exit', resolveExit));
    }
    await rm(workspace, { recursive: true, force: true });
  }
}

function processRss(pid) {
  const text = execFileSync('ps', ['-o', 'rss=', '-p', String(pid)], { encoding: 'utf8' });
  return Number.parseInt(text.trim(), 10) || 0;
}

function run(command, args) {
  return new Promise((resolveRun, rejectRun) => {
    const child = spawn(command, args, { cwd: process.cwd(), stdio: 'inherit' });
    child.once('error', rejectRun);
    child.once('exit', (code, signal) => {
      if (code === 0) resolveRun();
      else rejectRun(new Error(`${command} exited with ${signal ?? code}`));
    });
  });
}

async function waitForServer(child) {
  const lines = [];
  for await (const line of createInterface({ input: child.stdout })) {
    if (line.trim()) lines.push(line.trim());
    if (lines.length === 2) {
      return { address: lines[0], token: lines[1] };
    }
  }
  throw new Error('Studio server did not start');
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  await main();
}
