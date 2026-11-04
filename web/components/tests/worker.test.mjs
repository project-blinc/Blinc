import { test } from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import { readFile } from 'node:fs/promises';
import { envelope, valid, HOST_TYPES } from '../runtime/protocol.js';

// Exercise worker lifecycle and ordering without substituting a DOM widget for Blinc.
// Actual pixels and component callbacks are covered by the native recording gate.
async function harness({ failBoot = false, gateWarm = false } = {}) {
  const messages = [], calls = [], pending = new Map();
  let closed = false, next = 1, releaseWarm;
  const warm = gateWarm ? new Promise(resolve => { releaseWarm = resolve; }) : Promise.resolve();
  const runtime = {
    frame: () => { calls.push('frame'); return JSON.stringify({ frames: calls.filter(c => c === 'frame').length, backend: 'worker-webgpu', cursor: 'pointer' }); },
    warm_frame: async () => { await warm; return runtime.frame(); },
    control: (...args) => calls.push(['props', ...args]),
    input: (...args) => calls.push(['input', ...args]),
    resize: (...args) => calls.push(['resize', ...args]),
    theme: (...args) => calls.push(['theme', ...args]),
    apply_css: css => { calls.push(['css', css]); if (css === 'invalid') throw new Error('CSS: expected closing brace'); },
    free: () => calls.push('free'),
  };
  const scope = { onmessage: null, close() { closed = true; }, addEventListener() {} };
  const context = vm.createContext({ URL, console, self: scope,
    postMessage: message => messages.push(message),
    requestAnimationFrame: callback => { const id = next++; pending.set(id, callback); return id; },
    cancelAnimationFrame: id => pending.delete(id),
  });
  const protocol = new vm.SourceTextModule(await readFile(new URL('../runtime/protocol.js', import.meta.url), 'utf8'), { context });
  await protocol.link(() => {}); await protocol.evaluate();
  const fakeWasm = new vm.SyntheticModule(['default', 'create_runtime'], function () {
    this.setExport('default', async () => { if (failBoot) throw new Error('adapter unavailable'); });
    this.setExport('create_runtime', async (...args) => { calls.push(['create', ...args]); return runtime; });
  }, { context });
  await fakeWasm.link(() => {}); await fakeWasm.evaluate();
  const worker = new vm.SourceTextModule(await readFile(new URL('../runtime/worker.js', import.meta.url), 'utf8'), {
    context,
    initializeImportMeta: meta => { meta.url = 'https://blinc.test/components/releases/test/runtime/worker.js'; },
    importModuleDynamically: async () => fakeWasm,
  });
  await worker.link(() => protocol); await worker.evaluate();
  let seq = 0;
  const send = (type, payload = {}, extra = {}) => scope.onmessage({ data: { ...envelope('demo-1', seq++, type, payload), ...extra } });
  return { messages, calls, pending, send, releaseWarm: () => releaseWarm?.(), get closed() { return closed; },
    draw() { const [id, callback] = pending.entries().next().value; pending.delete(id); callback(); },
  };
}
const boot = { build: 'test', slug: 'button', width: 560, height: 320, dpr: 2, dark: true, family: 'hybrid', css: '', disabled: false, progress: 42 };

test('Preview warms exactly one completed frame and does not begin an animation loop', async () => {
  const h = await harness({ gateWarm: true });
  const loading = h.send('boot', boot);
  await Promise.resolve(); await Promise.resolve();
  assert.equal(h.messages.some(m => m.type === 'ready'), false);
  await h.send('pause');
  h.releaseWarm(); await loading;
  assert.deepEqual(h.messages.map(m => m.type), ['progress', 'progress', 'first-frame', 'ready']);
  assert.equal(h.calls.filter(c => c === 'frame').length, 1);
  assert.equal(h.pending.size, 0);
});

test('Live resumes, Preview cancels frames, and dispose releases the runtime', async () => {
  const h = await harness(); await h.send('boot', boot);
  await h.send('resume'); assert.equal(h.pending.size, 1);
  h.draw(); assert.equal(h.pending.size, 1);
  await h.send('pause'); assert.equal(h.pending.size, 0);
  const frames = h.calls.filter(c => c === 'frame').length;
  await h.send('dispose');
  assert.equal(h.closed, true);
  assert.equal(h.calls.filter(c => c === 'free').length, 1);
  await h.send('resume'); assert.equal(h.pending.size, 0);
  assert.equal(h.calls.filter(c => c === 'frame').length, frames);
});

test('Input transitions retain order and reject stale IDs, schema versions and sequences', async () => {
  const h = await harness(); await h.send('boot', boot);
  await h.send('input', { event: { kind: 'key-down', code: 65 }, modifiers: { shift: true } });
  await h.send('input', { event: { kind: 'text', text: 'A' } });
  await h.send('input', { event: { kind: 'text', text: 'old' } }, { instance: 'old-worker' });
  await h.send('input', { event: { kind: 'text', text: 'old' } }, { seq: 1 });
  await h.send('input', { event: { kind: 'text', text: 'old' } }, { version: 99 });
  const inputs = h.calls.filter(c => Array.isArray(c) && c[0] === 'input');
  assert.deepEqual(inputs.map(c => JSON.parse(c[1]).kind), ['key-down', 'text']);
  assert.equal(inputs[0][2], true);
  await h.send('boot', boot);
  assert.equal(h.messages.filter(m => m.type === 'ready').length, 1);
});

test('A GPU startup error cannot claim readiness or leave a render loop running', async () => {
  const h = await harness({ failBoot: true }); await h.send('boot', boot);
  assert.equal(h.messages.filter(m => m.type === 'error').length, 1);
  assert.match(h.messages.at(-1).payload.message, /adapter unavailable/);
  assert.equal(h.messages.some(m => m.type === 'ready'), false);
  assert.equal(h.pending.size, 0);
});

test('Disposing during warmup prevents a late first-frame or ready notification', async () => {
  const h = await harness({ gateWarm: true });
  const loading = h.send('boot', boot);
  await Promise.resolve(); await Promise.resolve();
  await h.send('dispose'); h.releaseWarm(); await loading;
  assert.equal(h.messages.some(m => m.type === 'ready'), false);
  assert.equal(h.pending.size, 0);
  assert.equal(h.closed, true);
  assert.equal(h.calls.filter(c => c === 'free').length, 1);
});

test('Protocol rejects malformed payloads and unknown message kinds', () => {
  assert.equal(valid(envelope('a', 0, 'input', []), 'a', HOST_TYPES), false);
  assert.equal(valid(envelope('a', 0, 'unknown'), 'a', HOST_TYPES), false);
  assert.equal(valid(envelope('a', -1, 'pause'), 'a', HOST_TYPES), false);
  assert.equal(valid(envelope('a', 0, 'pause'), 'a', HOST_TYPES), true);
});

test('Theme bundles and custom CSS reach the Blinc runtime as distinct inputs', async () => {
  const h = await harness();
  const css = '.cn-input:focus { outline-color: #ff8800; }';
  await h.send('boot', { ...boot, family: 'expressive', css });
  const create = h.calls.find(call => Array.isArray(call) && call[0] === 'create');
  assert.deepEqual(create.slice(-3), [true, 'expressive', css]);
  await h.send('theme', { dark: false });
  assert.deepEqual(h.calls.at(-1), ['theme', false, undefined]);
  await h.send('theme', { dark: true, family: 'restrained' });
  assert.deepEqual(h.calls.at(-1), ['theme', true, 'restrained']);
});

test('CSS edits refresh the existing runtime and invalid edits remain recoverable', async () => {
  const h = await harness(); await h.send('boot', boot);
  await h.send('css', { css: '.cn-button { color: var(--primary); }', request: 1 });
  assert.equal(h.messages.at(-1).type, 'css-result');
  assert.equal(h.messages.at(-1).payload.ok, true);
  assert.equal(h.pending.size, 1, 'a paused preview refreshes one frame');
  h.draw(); assert.equal(h.pending.size, 0);
  await h.send('css', { css: 'invalid', request: 2 });
  assert.equal(h.messages.at(-1).payload.ok, false);
  await h.send('css', { css: '', request: 3 });
  assert.equal(h.messages.at(-1).payload.ok, true);
  assert.equal(h.calls.filter(call => Array.isArray(call) && call[0] === 'create').length, 1);
  assert.equal(h.calls.includes('free'), false);
  assert.equal(h.messages.some(message => message.type === 'error'), false);
});
