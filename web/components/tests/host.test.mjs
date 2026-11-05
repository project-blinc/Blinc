import { test } from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import { readFile } from 'node:fs/promises';
import { envelope } from '../runtime/protocol.js';

async function host() {
  class Element {
    constructor() { this.style = {}; this.events = {}; this.rect = { left: 80, top: 120, width: 328, height: 360 }; }
    setAttribute() {}
    addEventListener(type, callback) { (this.events[type] ||= []).push(callback); }
    emit(type, event = {}) { for (const callback of this.events[type] || []) callback({ preventDefault() {}, ...event }); }
    getBoundingClientRect() { return this.rect; }
    replaceChildren(...children) { this.children = children; }
    focus() { this.emit('focus'); }
    setPointerCapture() { this.captured = true; }
    hasPointerCapture() { return this.captured; }
    releasePointerCapture() { this.captured = false; this.emit('lostpointercapture'); }
    transferControlToOffscreen() { return { offscreen: true }; }
  }
  const workers = [];
  class Worker {
    constructor() { this.messages = []; workers.push(this); }
    postMessage(message) { this.messages.push(message); }
    terminate() { this.terminated = true; }
  }
  const container = new Element();
  const document = { hidden: false, createElement: () => new Element() };
  const context = vm.createContext({ URL, console, document, Worker, HTMLCanvasElement: Element,
    crypto: { randomUUID: () => 'host-test' }, AbortController,
    devicePixelRatio: 2, performance: { now: () => 100 },
    window: { addEventListener() {} }, matchMedia: () => ({ addEventListener() {}, removeEventListener() {} }),
    setTimeout: () => 1, clearTimeout() {},
    ResizeObserver: class { observe() {} disconnect() {} },
    WheelEvent: { DOM_DELTA_LINE: 1, DOM_DELTA_PAGE: 2 },
  });
  const protocol = new vm.SourceTextModule(await readFile(new URL('../runtime/protocol.js', import.meta.url), 'utf8'), { context });
  await protocol.link(() => {});
  const module = new vm.SourceTextModule(await readFile(new URL('../runtime/host.js', import.meta.url), 'utf8'), {
    context, initializeImportMeta: meta => { meta.url = 'https://blinc.test/components/releases/test/runtime/host.js'; },
  });
  const media = new vm.SourceTextModule(await readFile(new URL('../runtime/media-host.js', import.meta.url), 'utf8'), { context });
  await media.link(() => {});
  await module.link(specifier => specifier.endsWith('media-host.js') ? media : protocol); await module.evaluate();
  const demo = new module.namespace.LiveDemo(container, { slug: 'button', title: 'Button', build: 'test', width: 560, height: 320 }, () => {});
  let seq = 0;
  return { demo, container, worker: workers[0],
    notify(type, payload = {}) { workers[0].onmessage({ data: envelope('host-test', seq++, type, payload) }); },
  };
}

test('The live surface follows the actual container and state messages do not synthesize pointer exits', async () => {
  const { demo, container, worker, notify } = await host();
  const boot = worker.messages[0];
  assert.equal(boot.payload.width, 328);
  assert.equal(boot.payload.height, 360);
  assert.equal(boot.payload.dpr, 2);
  demo.setVisible(true); notify('ready', { cursor: 'default' });
  const canvas = demo.canvas;
  canvas.emit('pointermove', { clientX: 180, clientY: 210 });
  const input = () => worker.messages.filter(message => message.type === 'input').map(message => message.payload.event);
  assert.deepEqual(JSON.parse(JSON.stringify(input())), [{ kind: 'pointer-move', x: 100, y: 90 }]);
  for (let i = 0; i < 12; i++) notify('state', { cursor: i % 2 ? 'pointer' : 'default' });
  assert.equal(input().length, 1, 'cursor reports must not inject hover or leave events');
  assert.equal(demo.canvas, canvas);
  container.rect = { ...container.rect, width: 285, height: 400 };
  demo.resize();
  assert.deepEqual(JSON.parse(JSON.stringify(worker.messages.at(-1).payload)), { width: 285, height: 400, dpr: 2 });
  canvas.emit('pointerleave');
  assert.equal(input().at(-1).kind, 'pointer-leave');
  demo.stop(); assert.equal(worker.terminated, true);
});

test('Pointer release keeps hover and text commits are forwarded once', async () => {
  const { demo, worker, notify } = await host();
  demo.setVisible(true); notify('ready');
  const position = { clientX: 180, clientY: 210, pointerId: 1, button: 0 };
  demo.canvas.emit('pointerdown', position);
  demo.canvas.emit('pointerup', position);
  const input = () => worker.messages.filter(message => message.type === 'input').map(message => message.payload.event);
  assert.deepEqual(input().map(event => event.kind), ['focus', 'pointer-move', 'pointer-down', 'pointer-up']);
  demo.editor.emit('compositionstart');
  demo.editor.emit('input', { isComposing: true, data: 'é' });
  demo.editor.emit('compositionend', { data: 'é' });
  demo.editor.emit('input', { inputType: 'insertFromComposition', data: 'é' });
  assert.deepEqual(input().filter(event => event.kind === 'text').map(event => event.text), ['é']);
  demo.stop();
});

test('in-canvas context menu, dragging and text selection suppress browser defaults', async () => {
  const { demo } = await host();
  for (const type of ['contextmenu', 'dragstart', 'selectstart']) {
    let prevented = false;
    (type === 'contextmenu' ? demo.container : demo.canvas).emit(type, { preventDefault() { prevented = true; } });
    assert.ok(prevented, type);
  }
});

test('Wheel timing uses the original DOM event timestamp', async () => {
  const { demo, worker, notify } = await host();
  demo.setVisible(true); notify('ready');
  for (const timeStamp of [233, 249, 281]) {
    demo.canvas.emit('wheel', { clientX: 180, clientY: 210, deltaX: 12, deltaY: 0, deltaMode: 0, timeStamp });
  }
  const wheel = worker.messages.filter(m => m.type === 'input' && m.payload.event.kind === 'wheel');
  assert.deepEqual(wheel.map(m => m.payload.timestamp), [233, 249, 281]);
  assert.deepEqual(wheel.map(m => m.payload.event.x), [-12, -12, -12]);
  demo.stop();
});
