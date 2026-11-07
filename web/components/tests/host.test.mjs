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
    focus() { if (!this.focused) { this.focused = true; this.emit('focus'); } }
    blur() { if (this.focused) { this.focused = false; this.emit('blur'); } }
    setPointerCapture() { this.captured = true; }
    hasPointerCapture() { return this.captured; }
    releasePointerCapture() { this.captured = false; this.emit('lostpointercapture'); }
    transferControlToOffscreen() { return { offscreen: true }; }
  }
  const workers = [];
  const timers = new Map(); let nextTimer = 1;
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
    setTimeout: callback => { const id = nextTimer++; timers.set(id, callback); return id; }, clearTimeout(id) { timers.delete(id); },
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
  return { demo, container, worker: workers[0], timers,
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

test('A one-finger drag scrolls and its lift ends the gesture, while a mouse drag sends no touch input', async () => {
  const { demo, worker, notify } = await host();
  demo.setVisible(true); notify('ready');
  const input = () => worker.messages.filter(message => message.type === 'input').map(message => JSON.parse(JSON.stringify(message.payload.event)));
  const touch = (x, y) => ({ clientX: 80 + x, clientY: 120 + y, pointerId: 7, pointerType: 'touch', button: 0 });
  demo.canvas.emit('pointerdown', touch(100, 200));
  demo.canvas.emit('pointermove', touch(100, 170));
  demo.canvas.emit('pointermove', touch(104, 150));
  demo.canvas.emit('pointerup', touch(104, 150));
  const kinds = input().map(event => event.kind);
  assert.deepEqual(kinds, ['focus', 'touch-start', 'pointer-move', 'pointer-down', 'pointer-move', 'touch-scroll', 'pointer-move', 'touch-scroll', 'pointer-up', 'touch-end']);
  assert.deepEqual(input().filter(event => event.kind === 'touch-scroll'), [{ kind: 'touch-scroll', dx: 0, dy: -30 }, { kind: 'touch-scroll', dx: 4, dy: -20 }]);
  const before = input().length;
  const mouse = { clientX: 180, clientY: 210, pointerId: 1, pointerType: 'mouse', button: 0 };
  demo.canvas.emit('pointerdown', mouse);
  demo.canvas.emit('pointermove', { ...mouse, clientY: 250 });
  demo.canvas.emit('pointerup', mouse);
  assert.ok(input().slice(before).every(event => !event.kind.startsWith('touch')), 'a mouse never produces touch input');
  // A cancelled touch ends the gesture once, not twice.
  demo.canvas.emit('pointerdown', touch(50, 50));
  demo.canvas.emit('pointercancel', touch(50, 50));
  demo.canvas.emit('pointerup', touch(50, 50));
  assert.equal(input().filter(event => event.kind === 'touch-end').length, 2);
});

test('A tap raises the keyboard and drops it only when nothing took focus; a drag and a report never flip it off', async () => {
  const { demo, notify, timers } = await host();
  demo.setVisible(true); notify('ready');
  const at = (x, y) => ({ clientX: 80 + x, clientY: 120 + y, pointerId: 3, pointerType: 'touch', button: 0 });
  const runTimers = () => { for (const [id, callback] of [...timers]) { timers.delete(id); callback(); } };
  assert.equal(demo.editor.inputMode, 'none', 'no keyboard before any tap');
  // A tap on a text input: the demo then reports it is editing, and the keyboard stays.
  demo.canvas.emit('pointerdown', at(100, 100)); demo.canvas.emit('pointerup', at(100, 100));
  assert.equal(demo.editor.inputMode, 'text', 'the tap raises it inside the gesture');
  notify('state', { editable: false });
  notify('state', { editable: true });
  runTimers();
  assert.equal(demo.editor.inputMode, 'text', 'an editable demo keeps its keyboard');
  // A report that nothing is editable does not drop it by itself.
  notify('state', { editable: false });
  assert.equal(demo.editor.inputMode, 'text');
  // A tap that lands on nothing editable drops it once the delay has passed.
  demo.canvas.emit('pointerdown', at(20, 20)); demo.canvas.emit('pointerup', at(20, 20));
  assert.equal(demo.editor.inputMode, 'text');
  runTimers();
  assert.equal(demo.editor.inputMode, 'none');
  // A drag is a scroll: it does not raise the keyboard.
  demo.canvas.emit('pointerdown', at(50, 200)); demo.canvas.emit('pointermove', at(50, 120)); demo.canvas.emit('pointerup', at(50, 120));
  assert.equal(demo.editor.inputMode, 'none');
});
