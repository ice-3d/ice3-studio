import assert from 'node:assert/strict';
import test from 'node:test';
import { StudioClient } from '../src/api/StudioClient';

test('preview prefers the advertised packet URL', async () => {
  Object.defineProperty(globalThis, 'window', {
    configurable: true,
    value: { location: { search: '' } },
  });
  Object.defineProperty(globalThis, 'localStorage', {
    configurable: true,
    value: { getItem: () => null },
  });
  Object.defineProperty(globalThis, '__STUDIO_TOKEN__', {
    configurable: true,
    value: '',
  });
  const client = new StudioClient();
  Object.defineProperty(client, 'rpc', {
    value: async () => ({
      packetUrl: '/preview-packets/7',
      previewPath: '/tmp/preview.glb',
    }),
  });

  const preview = await client.preview(7);

  assert.deepEqual(preview, {
    revision: 7,
    url: '/preview-packets/7',
    packetUrl: '/preview-packets/7',
    glbUrl: '/previews/7',
  });
});

test('preview falls back to the GLB route when packet support is absent', async () => {
  Object.defineProperty(globalThis, 'window', {
    configurable: true,
    value: { location: { search: '' } },
  });
  Object.defineProperty(globalThis, 'localStorage', {
    configurable: true,
    value: { getItem: () => null },
  });
  Object.defineProperty(globalThis, '__STUDIO_TOKEN__', {
    configurable: true,
    value: '',
  });
  const client = new StudioClient();
  Object.defineProperty(client, 'rpc', {
    value: async () => ({ packetUrl: null }),
  });

  const preview = await client.preview(8);

  assert.deepEqual(preview, {
    revision: 8,
    url: '/previews/8',
    packetUrl: null,
    glbUrl: '/previews/8',
  });
});

test('candidate preview prefers its packet URL', async () => {
  Object.defineProperty(globalThis, 'window', {
    configurable: true,
    value: { location: { search: '' } },
  });
  Object.defineProperty(globalThis, 'localStorage', {
    configurable: true,
    value: { getItem: () => null },
  });
  Object.defineProperty(globalThis, '__STUDIO_TOKEN__', {
    configurable: true,
    value: '',
  });
  const client = new StudioClient();
  Object.defineProperty(client, 'rpc', {
    value: async () => ({
      url: `/previews/candidates/${'a'.repeat(64)}`,
      packetUrl: `/preview-packets/candidates/${'a'.repeat(64)}`,
    }),
  });

  const preview = await client.candidatePreview('a'.repeat(64));

  assert.equal(
    preview.url,
    `/preview-packets/candidates/${'a'.repeat(64)}`,
  );
});
