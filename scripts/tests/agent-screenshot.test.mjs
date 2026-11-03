import test from 'node:test';
import assert from 'node:assert/strict';

import {
  buildScreenshotTarget,
  parseArguments,
} from '../agent-screenshot.mjs';

test('builds a viewer url with view and target overrides', () => {
  const target = buildScreenshotTarget({
    baseUrl: 'http://127.0.0.1:8149',
    token: 'session-token',
    view: '45,26,360',
    target: '300,4,0',
  });
  const url = new URL(target.url);
  assert.equal(url.origin, 'http://127.0.0.1:8149');
  assert.equal(url.searchParams.get('token'), 'session-token');
  assert.equal(url.searchParams.get('view'), '45,26,360');
  assert.equal(url.searchParams.get('target'), '300,4,0');
});

test('omits absent view, target, and token parameters', () => {
  const target = buildScreenshotTarget({ baseUrl: 'http://127.0.0.1:8120' });
  const url = new URL(target.url);
  assert.equal(url.searchParams.has('token'), false);
  assert.equal(url.searchParams.has('view'), false);
  assert.equal(url.searchParams.has('target'), false);
});

test('passes through a full url without rewrites', () => {
  const full = 'http://127.0.0.1:8120/?token=abc&view=20,34,540';
  const target = buildScreenshotTarget({ url: full });
  assert.equal(target.url, full);
  assert.equal(target.baseUrl, null);
});

test('rejects malformed view and target values', () => {
  for (const bad of ['45,26', 'a,b,c', '45,26,360,1', '']) {
    assert.throws(
      () => buildScreenshotTarget({ baseUrl: 'http://127.0.0.1:8120', view: bad }),
      /--view/,
    );
    assert.throws(
      () => buildScreenshotTarget({ baseUrl: 'http://127.0.0.1:8120', target: bad }),
      /--target/,
    );
  }
});

test('parses argv flags with defaults and validates conflicts', () => {
  const options = parseArguments(['--view', '40,20,120', '--out', '/tmp/shot.png']);
  assert.equal(options.view, '40,20,120');
  assert.equal(options.out, '/tmp/shot.png');
  assert.equal(options.baseUrl, 'http://127.0.0.1:8120');
  assert.equal(options.width, 1600);
  assert.equal(options.channel, 'chrome');

  assert.throws(
    () => parseArguments(['--url', 'http://x/?token=1', '--view', '1,2,3']),
    /cannot be combined/,
  );
  assert.throws(() => parseArguments(['--nope']), /unknown option/);
  assert.throws(() => parseArguments(['--view']), /missing value/);
});
