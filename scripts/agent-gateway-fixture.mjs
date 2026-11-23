import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';

export async function startAgentGateway({
  studioUrl,
  token,
  projectId,
}) {
  const child = spawn(
    'cargo',
    [
      'run',
      '-q',
      '-p',
      'studio-cli',
      '--bin',
      'agent-gateway-fixture',
      '--',
      '--studio-url',
      studioUrl,
      '--token',
      token,
      '--project',
      projectId,
    ],
    {
      cwd: process.cwd(),
      stdio: ['ignore', 'pipe', 'inherit'],
    },
  );
  const reader = createInterface({ input: child.stdout });
  const url = await new Promise((resolve, reject) => {
    reader.once('line', (line) => {
      const prefix = 'AGENT_GATEWAY_URL=';
      if (line.startsWith(prefix)) resolve(line.slice(prefix.length));
      else reject(new Error(`Unexpected Gateway fixture output: ${line}`));
    });
    child.once('error', reject);
    child.once('exit', (code) => {
      reject(new Error(`Gateway fixture exited before ready: ${code}`));
    });
  });
  return {
    url,
    close() {
      reader.close();
      child.kill('SIGTERM');
    },
  };
}
