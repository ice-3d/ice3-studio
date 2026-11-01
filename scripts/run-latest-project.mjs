import { spawn } from 'node:child_process';
import { access, readdir, readFile, stat } from 'node:fs/promises';
import { createInterface } from 'node:readline';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const studioRoot = resolve(scriptDir, '..');

export function selectLatestProject(projects) {
  if (!Array.isArray(projects) || projects.length === 0) {
    throw new Error('No Studio projects were found');
  }
  return [...projects].sort((left, right) => (
    right.mtimeMs - left.mtimeMs
    || right.currentRevision - left.currentRevision
    || right.projectId.localeCompare(left.projectId, 'en')
  ))[0];
}

export async function discoverProjects(projectsRoot) {
  const entries = await readdir(projectsRoot, { withFileTypes: true });
  const projects = [];
  for (const entry of entries) {
    if (!entry.isDirectory()) continue;
    const projectFile = join(projectsRoot, entry.name, 'project.json');
    try {
      const [projectJson, fileStat] = await Promise.all([
        readFile(projectFile, 'utf8'),
        stat(projectFile),
      ]);
      const project = JSON.parse(projectJson);
      projects.push({
        projectId: String(project.projectId ?? entry.name),
        currentRevision: Number(project.currentRevision ?? 0),
        mtimeMs: fileStat.mtimeMs,
        projectFile,
      });
    } catch (error) {
      if (error?.code !== 'ENOENT') throw error;
    }
  }
  return projects;
}

async function run() {
  const projectsRoot = resolve(
    process.env.STUDIO_PROJECTS_DIR ?? join(studioRoot, 'projects'),
  );
  const latest = selectLatestProject(await discoverProjects(projectsRoot));
  const viewerDist = resolve(
    process.env.STUDIO_VIEWER_DIR ?? join(studioRoot, 'apps/viewer/dist'),
  );
  await access(join(viewerDist, 'index.html'));

  const port = String(process.env.STUDIO_PORT ?? 8149);
  const token = process.env.STUDIO_SESSION_TOKEN ?? 'wastewater-review';
  const child = spawn(
    process.env.STUDIO_CLI ?? 'cargo',
    process.env.STUDIO_CLI
      ? [
        'serve',
        '--workspace', projectsRoot,
        '--project', latest.projectId,
        '--port', port,
        '--write-mode', 'review-required',
      ]
      : [
        'run',
        '--release',
        '-p',
        'studio-cli',
        '--',
        'serve',
        '--workspace', projectsRoot,
        '--project', latest.projectId,
        '--port', port,
        '--write-mode', 'review-required',
      ],
    {
      cwd: studioRoot,
      env: {
        ...process.env,
        STUDIO_SESSION_TOKEN: token,
        STUDIO_VIEWER_DIR: viewerDist,
      },
      stdio: ['inherit', 'pipe', 'inherit'],
    },
  );

  let startupLines = 0;
  const reader = createInterface({ input: child.stdout });
  reader.on('line', (line) => {
    const text = line.trim();
    if (!text) return;
    startupLines += 1;
    if (startupLines <= 2) console.log(text);
    if (startupLines === 2) {
      const origin = text.startsWith('http')
        ? text
        : `http://127.0.0.1:${port}`;
      console.log(
        `Studio Viewer (${latest.projectId}, revision ${latest.currentRevision}): `
        + `${origin}/?token=${encodeURIComponent(token)}`,
      );
    }
  });

  for (const signal of ['SIGINT', 'SIGTERM']) {
    process.once(signal, () => child.kill(signal));
  }
  child.once('error', (error) => {
    console.error(error);
    process.exitCode = 1;
  });
  child.once('exit', (code, signal) => {
    reader.close();
    process.exitCode = signal ? 1 : (code ?? 1);
  });
}

if (
  process.argv[1]
  && import.meta.url === pathToFileURL(resolve(process.argv[1])).href
) {
  run().catch((error) => {
    console.error(error);
    process.exitCode = 1;
  });
}
