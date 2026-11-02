#!/usr/bin/env node
import { copyFile, readFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = dirname(fileURLToPath(import.meta.url));
const inputIndex = process.argv.indexOf('--input');
const outputIndex = process.argv.indexOf('--output');
if (inputIndex < 0 || outputIndex < 0) process.exit(2);

const marker = await readFile(process.argv[inputIndex + 1], 'utf8');
if (!marker.includes('ICE3_JT_CONVERTER_CONTRACT_V1')) process.exit(3);
const name = marker.includes('EMPTY=1')
  ? 'empty.raw.glb'
  : marker.includes('ORIGIN=')
    ? 'large-origin.raw.glb'
    : 'synthetic-contract.raw.glb';
await copyFile(join(root, name), process.argv[outputIndex + 1]);
