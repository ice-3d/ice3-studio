import { spawn } from 'node:child_process';

const toolchainCli = process.env.ICE3_TOOLCHAIN_CLI;
if (!toolchainCli) {
  console.error('ICE3_TOOLCHAIN_CLI is not configured');
  process.exit(2);
}

const child = spawn(process.execPath, [toolchainCli, ...process.argv.slice(2)], {
  stdio: 'inherit',
});
child.once('error', (error) => {
  console.error(error.message);
  process.exit(1);
});
child.once('exit', (code, signal) => {
  process.exit(signal ? 1 : (code ?? 1));
});
