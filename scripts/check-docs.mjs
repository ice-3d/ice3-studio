#!/usr/bin/env node
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { dirname, extname, join, relative, resolve } from 'node:path';

const ROOT = resolve(import.meta.dirname, '..');
const DOCS = join(ROOT, 'docs');
const INDEX = join(DOCS, 'README.md');

const STATUSES = new Set([
  'Implemented',
  'Historical',
  'Approved',
  'Proposed',
  'Deprecated',
  'Active',
]);
const REQUIRED_FIELDS = ['Document ID', 'Status', 'Owner'];
const ROOT_DOCS = ['README.md', 'AGENTS.md', 'THIRD_PARTY_NOTICES.md'];

function walk(dir) {
  const found = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.name.startsWith('.') || entry.name === 'node_modules') continue;
    const full = join(dir, entry.name);
    if (entry.isDirectory()) found.push(...walk(full));
    else if (extname(entry.name) === '.md') found.push(full);
  }
  return found;
}

function proseOnly(text) {
  return text
    .split(/^```/m)
    .filter((_, index) => index % 2 === 0)
    .join('\n');
}

const indexText = readFileSync(INDEX, 'utf8');

const indexTargets = [...proseOnly(indexText).matchAll(/]\(([^)\s]+\.md(?:#[^)]*)?)\)/g)]
  .map((match) => decodeURIComponent(match[1].split('#')[0]))
  .filter((path) => !/^https?:/.test(path))
  .map((path) => resolve(DOCS, path))
  .filter(existsSync);

const files = [
  ...new Set([
    ...walk(DOCS),
    ...ROOT_DOCS.map((name) => join(ROOT, name)),
    ...indexTargets,
  ]),
].filter(existsSync);

const errors = [];
const declared = new Map();

for (const file of files) {
  const rel = relative(ROOT, file);
  const text = readFileSync(file, 'utf8');
  const head = text.split('\n').slice(0, 14).join('\n');

  for (const field of REQUIRED_FIELDS) {
    const pattern = new RegExp(`^>\\s*${field}:\\s*\\S`, 'm');
    if (!pattern.test(head)) {
      errors.push(`${rel}: missing front-matter field "${field}"`);
    }
  }

  const status = head.match(/^>\s*Status:\s*(\S+)/m);
  if (status && !STATUSES.has(status[1])) {
    errors.push(
      `${rel}: unknown Status "${status[1]}" (allowed: ${[...STATUSES].join(', ')})`,
    );
  }

  const id = text.match(/^>\s*Document ID:\s*(\S+)/m);
  if (id) {
    const owners = declared.get(id[1]) ?? [];
    owners.push(rel);
    declared.set(id[1], owners);
    if (rel !== 'docs/README.md' && !indexText.includes(id[1])) {
      errors.push(`${rel}: ${id[1]} is not listed in docs/README.md`);
    }
  }
}

for (const [id, owners] of declared) {
  if (owners.length > 1) {
    errors.push(`duplicate Document ID ${id}: ${owners.join(', ')}`);
  }
}

for (const file of files.filter((path) => path.startsWith(DOCS))) {
  const rel = relative(ROOT, file);
  const text = proseOnly(readFileSync(file, 'utf8'));
  for (const match of text.matchAll(/]\(([^)\s]+)\)/g)) {
    const target = match[1];
    if (/^(https?:|mailto:|#)/.test(target)) continue;
    const path = decodeURIComponent(target.split('#')[0].split('?')[0]);
    if (!path) continue;
    if (!existsSync(resolve(dirname(file), path))) {
      errors.push(`${rel}: broken relative link -> ${target}`);
    }
  }
}

const indexed = new Set(
  [...indexText.matchAll(/ICE3-STUDIO-[A-Z]+-\d{3}/g)].map((match) => match[0]),
);
for (const id of indexed) {
  if (!declared.has(id)) {
    errors.push(`docs/README.md: references ${id} but no document declares it`);
  }
}

if (errors.length > 0) {
  console.error(`Documentation contract: ${errors.length} problem(s)`);
  for (const error of errors) console.error(`  - ${error}`);
  process.exit(1);
}

console.log(
  `Documentation contract: ${files.length} file(s), ${declared.size} document IDs, all indexed and linked`,
);
