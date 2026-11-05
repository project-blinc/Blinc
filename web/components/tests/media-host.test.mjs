import { test } from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import { readFile } from 'node:fs/promises';

async function setup(video = false) {
  const frames = new Map(), messages = [];
  let id = 0;
  const media = {
    paused: true, ended: false, muted: false, volume: 1, currentTime: 0, duration: 20, videoWidth: 0,
    buffered: { length: 0 }, events: {},
    addEventListener(name, fn) { (this.events[name] ||= []).push(fn); },
    emit(name) { for (const fn of this.events[name] || []) fn(); },
    play() { this.paused = false; this.emit('play'); return Promise.resolve(); },
    pause() { this.paused = true; this.emit('pause'); },
    requestVideoFrameCallback() { return 1; }, cancelVideoFrameCallback() {},
    removeAttribute() {}, load() {}, remove() {},
  };
  const context = vm.createContext({
    document: { createElement: tag => tag === 'video' ? media : { getContext() { return {}; } }, body: { append() {} } },
    requestAnimationFrame: fn => { frames.set(++id, fn); return id; },
    cancelAnimationFrame: key => frames.delete(key),
  });
  const module = new vm.SourceTextModule(await readFile(new URL('../runtime/media-host.js', import.meta.url), 'utf8'), { context });
  await module.link(() => {}); await module.evaluate();
  const host = new module.namespace.MediaHost((type, payload) => messages.push({ type, payload }), error => { throw new Error(error); });
  host.video = video; host.request({ id: 1, operation: 'create' });
  const tick = () => { const current = [...frames.values()]; frames.clear(); for (const fn of current) fn(); };
  return { host, media, frames, messages, tick };
}

test('audio publishes live playback time each frame and stops its clock on pause and disposal', async () => {
  const { host, media, frames, messages, tick } = await setup();
  assert.equal(frames.size, 0);
  host.request({ id: 1, operation: 'play' });
  assert.equal(frames.size, 1);
  media.currentTime = 0.15; tick();
  assert.equal(messages.at(-1).payload.position, 150);
  assert.equal(frames.size, 1);
  host.request({ id: 1, operation: 'pause' });
  assert.equal(frames.size, 0);
  const count = messages.length; tick(); assert.equal(messages.length, count);
  host.request({ id: 1, operation: 'play' });
  media.currentTime = 2; tick();
  assert.equal(messages.at(-1).payload.position, 2000);
  host.stop(); assert.equal(frames.size, 0);
});

test('video retains its decoded-frame clock without a second animation loop', async () => {
  const { host, frames } = await setup(true);
  host.request({ id: 1, operation: 'play' });
  assert.equal(frames.size, 0);
  host.stop();
});

test('video starts paused and audible, with volume controlled by the player', async () => {
  const { host, media } = await setup(true);
  assert.equal(media.paused, true);
  assert.equal(media.muted, false);
  assert.equal(media.volume, 1);
  host.request({ id: 1, operation: 'play' });
  assert.equal(media.paused, false);
  for (const volume of [0.35, 0, 1]) {
    host.request({ id: 1, operation: 'volume', value: volume });
    assert.equal(media.volume, volume);
    assert.equal(media.muted, false, 'the host must not override the player volume');
  }
  host.pause();
  assert.equal(media.paused, true);
  host.stop();
});
