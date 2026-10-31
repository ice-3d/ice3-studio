import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  mkdir,
  mkdtemp,
  readFile,
  rename,
  rm,
  writeFile,
} from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createInterface } from 'node:readline';
import { pathToFileURL } from 'node:url';
import { buildIndustrialScenePlan } from '../fixtures/large-industrial-site/operations.mjs';

export function verifyAppliedScene(stats, expected) {
  if (stats.sourceNodes < expected.primaryObjects) {
    throw new Error('source object count is below the accepted scale');
  }
  if (stats.uniqueMeshes > 1_200) {
    throw new Error(
      `equipment definitions did not share evaluated meshes: ${JSON.stringify(stats)}`,
    );
  }
  if (
    stats.effectiveTriangles < expected.minTriangles
    || stats.effectiveTriangles > expected.maxTriangles
  ) {
    throw new Error(`effective triangle count outside budget: ${stats.effectiveTriangles}`);
  }
  if (!Number.isInteger(stats.uniqueTriangles) || stats.uniqueTriangles <= 0) {
    throw new Error('unique triangle count is invalid');
  }
  if (stats.effectiveTriangles < stats.uniqueTriangles * 4) {
    throw new Error('shared geometry reuse is insufficient');
  }
}

async function main() {
  const root = resolve('.');
  const workspace = await mkdtemp(join(tmpdir(), 'ice3-studio-large-industrial-'));
  let server;
  try {
    await run('cargo', ['build', '--release', '-p', 'studio-cli']);
    const binary = resolve(
      'target/release',
      process.platform === 'win32' ? 'studio-cli.exe' : 'studio-cli',
    );
    server = spawn(
      binary,
      [
        'serve',
        '--workspace',
        workspace,
        '--project',
        'large-industrial-bootstrap',
        '--port',
        '0',
        '--write-mode',
        'autonomous',
      ],
      {
        cwd: root,
        env: {
          ...process.env,
          STUDIO_SESSION_TOKEN: 'large-industrial-fixture',
          STUDIO_VIEWER_DIR: resolve('apps/viewer/dist'),
        },
        stdio: ['ignore', 'pipe', 'inherit'],
      },
    );
    const { address, token } = await waitForServer(server);
    let requestId = 0;
    const rpc = async (method, params = {}) => {
      requestId += 1;
      const response = await fetch(`http://${address}/rpc`, {
        method: 'POST',
        headers: {
          authorization: `Bearer ${token}`,
          'content-type': 'application/json',
        },
        body: JSON.stringify({
          jsonrpc: '2.0',
          id: requestId,
          method,
          params,
        }),
      });
      const envelope = await response.json();
      if (envelope.error) {
        throw new Error(`${method}: ${envelope.error.message}`);
      }
      return envelope.result;
    };

    const plan = buildIndustrialScenePlan();
    await rpc('project.create', { projectId: plan.projectId });
    const dryRun = await rpc('document.apply', {
      ...plan.transactions[0],
      dryRun: true,
      idempotencyKey: 'large-industrial-dry-run',
    });
    if (!dryRun.dryRun || dryRun.revision !== 0) {
      throw new Error('large-industrial dry run changed the revision');
    }

    let committed;
    for (const transaction of plan.transactions) {
      committed = await rpc('document.apply', transaction);
    }
    if (!committed || committed.revision !== plan.transactions.length) {
      throw new Error('large-industrial plan did not commit every transaction');
    }

    const document = await rpc('document.get');
    const primaryNodes = document.scene.nodes.filter(
      (node) => node.content?.type === 'component_instance',
    );
    if (primaryNodes.length !== plan.expected.primaryObjects) {
      throw new Error(`primary object count mismatch: ${primaryNodes.length}`);
    }
    const stats = await rpc('evaluation.stats');
    verifyAppliedScene(stats, plan.expected);

    const exported = await rpc('export.sourceGlb');
    const sourceBytes = await readFile(exported.sourcePath);
    const revisionBeforeReopen = document.revision;
    const idsBeforeReopen = primaryNodes.map((node) => node.id).sort();

    await rpc('project.close');
    await rpc('project.open', { projectId: plan.projectId });
    const reopened = await rpc('document.get');
    const reopenedStats = await rpc('evaluation.stats');
    const reopenedPrimaryIds = reopened.scene.nodes
      .filter((node) => node.content?.type === 'component_instance')
      .map((node) => node.id)
      .sort();
    if (
      reopened.revision !== revisionBeforeReopen
      || JSON.stringify(reopenedPrimaryIds) !== JSON.stringify(idsBeforeReopen)
      || reopenedStats.effectiveTriangles !== stats.effectiveTriangles
      || reopenedStats.uniqueTriangles !== stats.uniqueTriangles
    ) {
      throw new Error('large-industrial scene changed after reload');
    }

    const report = {
      projectId: plan.projectId,
      revision: reopened.revision,
      primaryObjects: primaryNodes.length,
      operations: plan.transactions.reduce(
        (sum, transaction) => sum + transaction.operations.length,
        0,
      ),
      stats,
      reopenedStats,
      sourceGlbBytes: sourceBytes.byteLength,
      sourceGlbSha256: createHash('sha256').update(sourceBytes).digest('hex'),
      topology: plan.expected.topology,
    };
    await mkdir('output', { recursive: true });
    const destination = resolve('output/large-industrial-fixture.json');
    const temporary = `${destination}.tmp`;
    await writeFile(temporary, `${JSON.stringify(report, null, 2)}\n`);
    await rename(temporary, destination);
    console.log(JSON.stringify(report, null, 2));
  } finally {
    if (server) {
      server.kill('SIGTERM');
      await new Promise((resolveExit) => {
        server.once('exit', resolveExit);
      });
    }
    await rm(workspace, { recursive: true, force: true });
  }
}

function run(command, args) {
  return new Promise((resolveRun, reject) => {
    const child = spawn(command, args, {
      cwd: process.cwd(),
      env: process.env,
      stdio: 'inherit',
    });
    child.once('error', reject);
    child.once('exit', (code, signal) => {
      if (code === 0) {
        resolveRun();
      } else {
        reject(new Error(`${command} exited with ${signal ?? code}`));
      }
    });
  });
}

async function waitForServer(child) {
  const lines = createInterface({ input: child.stdout });
  const startup = [];
  for await (const line of lines) {
    if (line.trim()) startup.push(line.trim());
    if (startup.length === 2) break;
  }
  if (startup.length !== 2) throw new Error('Studio server did not start');
  return { address: startup[0], token: startup[1] };
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  await main();
}
