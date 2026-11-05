import { test } from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import { readFile } from 'node:fs/promises';

async function page(options = {}) {
  class Element {
    constructor(data = {}) { this.dataset = data; this.events = {}; this.attributes = {}; this.value = ''; this.parentElement = {}; }
    addEventListener(name, callback) { (this.events[name] ||= []).push(callback); }
    emit(name, event = {}) { for (const callback of this.events[name] || []) callback(event); }
    setAttribute(name, value) { this.attributes[name] = value; }
    focus() {}
  }
  const nodes = Object.fromEntries(['component-config', 'live-status', 'live-fallback', 'retry-live', 'live-canvas', 'demo-dark', 'scheme-label', 'demo-disabled', 'demo-progress', 'toggle-animation', 'reset-demo', 'custom-css', 'css-highlight', 'css-status', 'apply-css', 'clear-css'].map(id => [`#${id}`, new Element()]));
  nodes['#custom-css'].value = '.cn-input { border-radius: 16px; }';
  nodes['#component-config'].textContent = JSON.stringify({ slug: 'input', title: 'Input', build: 'test', width: 560, height: 320, ...options });
  nodes['#demo-dark'].checked = true;
  nodes['#demo-progress'].value = '42';
  const tabs = ['preview', 'live', 'code'].map(tab => new Element({ tab }));
  const panels = ['preview', 'live', 'code'].map(tab => Object.assign(new Element(), { id: `panel-${tab}` }));
  const themes = ['hybrid', 'restrained', 'expressive'].map(themeFamily => new Element({ themeFamily }));
  nodes['.controls'] = new Element(); nodes['.css-editor'] = new Element();
  nodes['.example'] = { style: { setProperty() {} }, querySelector: selector => nodes[selector], querySelectorAll: selector => ({ '[role="tab"]': tabs, '[role="tabpanel"]': panels, '[data-animation]': [] })[selector] || [] };
  const sessions = [];
  class LiveDemo {
    constructor(stage, options, onStatus) { this.options = options; this.onStatus = onStatus; sessions.push(this); }
    stop() { this.stopped = true; }
    setVisible(visible) { this.visible = visible; }
    send(type, payload) { (this.messages ||= []).push({ type, payload }); }
  }
  const document = {
    hidden: false, documentElement: { dataset: { scheme: 'dark' } }, body: { classList: { add() {} } }, addEventListener() {},
    querySelector: selector => nodes[selector],
    querySelectorAll: selector => ({ '[role="tab"]': tabs, '[role="tabpanel"]': panels, '[data-theme-family]': themes, '[data-animation]': [] })[selector] || [],
  };
  const context = vm.createContext({ document, console, URLSearchParams, location: { search: '' }, window: { addEventListener() {} }, matchMedia: () => ({ matches: false, addEventListener() {} }) });
  const host = new vm.SyntheticModule(['LiveDemo'], function () { this.setExport('LiveDemo', LiveDemo); }, { context });
  const code = new vm.SyntheticModule([], () => {}, { context });
  const syntax = new vm.SyntheticModule(['numbered'], function () { this.setExport('numbered', value => value); }, { context });
  const detail = new vm.SourceTextModule(await readFile(new URL('../detail.js', import.meta.url), 'utf8'), { context });
  await detail.link(specifier => specifier.endsWith('host.js') ? host : specifier.endsWith('format.js') ? syntax : code);
  await detail.evaluate();
  return { nodes, themes, tabs, sessions };
}

test('CSS applies in place, survives theme changes, and resets without lingering rules', async () => {
  const { nodes, themes, tabs, sessions } = await page();
  const runtime = sessions[0]; runtime.ready = true;
  tabs[1].emit('click');
  const editor = nodes['#custom-css'];
  editor.value = '.cn-input { background: var(--surface); }';
  editor.emit('input');
  assert.equal(sessions.length, 1, 'typing must not restart the runtime');
  nodes['#apply-css'].emit('click');
  assert.equal(sessions.length, 1, 'Apply must retain the runtime');
  assert.deepEqual(JSON.parse(JSON.stringify(runtime.messages.at(-1))), { type: 'css', payload: { css: editor.value, request: 1 } });
  runtime.onStatus('css-result', { request: 1, ok: true });
  assert.equal(nodes['#css-status'].textContent, 'Applied');
  themes[2].emit('click');
  assert.deepEqual(JSON.parse(JSON.stringify(runtime.messages.at(-1))), { type: 'theme', payload: { dark: true, family: 'expressive' } });
  nodes['#clear-css'].emit('click');
  assert.equal(runtime.messages.at(-1).payload.css, '');
  runtime.onStatus('css-result', { request: 2, ok: true });
  assert.equal(nodes['#css-status'].textContent, 'Default styles');
  assert.equal(sessions.length, 1);
  assert.equal(runtime.stopped, undefined);
});

test('CSS errors preserve the runtime and draft, and stale replies cannot overwrite a newer edit', async () => {
  const { nodes, sessions } = await page();
  const runtime = sessions[0]; runtime.ready = true;
  const editor = nodes['#custom-css'];
  editor.value = '.cn-input { background:';
  let prevented = false;
  editor.emit('keydown', { key: 'Enter', ctrlKey: true, preventDefault() { prevented = true; } });
  assert.equal(prevented, true);
  runtime.onStatus('css-result', { request: 1, ok: false, message: 'CSS: expected closing brace' });
  assert.equal(nodes['#css-status'].dataset.error, 'true');
  assert.match(nodes['#css-status'].textContent, /closing brace/);
  assert.equal(editor.value, '.cn-input { background:');
  editor.value = '.cn-input { color: var(--text-primary); }'; editor.emit('input');
  nodes['#apply-css'].emit('click');
  runtime.onStatus('css-result', { request: 1, ok: false, message: 'old error' });
  assert.equal(nodes['#css-status'].textContent, 'Applying…');
  runtime.onStatus('css-result', { request: 2, ok: true });
  assert.equal(nodes['#css-status'].textContent, 'Applied');
  assert.equal(nodes['#css-status'].dataset.error, undefined);
  assert.equal(sessions.length, 1);
});

test('CSS entered during warmup applies to the original worker once ready', async () => {
  const { nodes, sessions } = await page();
  nodes['#apply-css'].emit('click');
  assert.equal(sessions.length, 1);
  sessions[0].ready = true;
  sessions[0].onStatus('ready', { backend: 'worker-webgpu' });
  assert.equal(sessions[0].messages.at(-1).type, 'css');
  assert.equal(sessions[0].messages.at(-1).payload.css, nodes['#custom-css'].value);
});

test('The scheme switch updates the current theme without restarting the demo', async () => {
  const { nodes, sessions } = await page();
  sessions[0].ready = true;
  nodes['#demo-dark'].checked = false;
  nodes['#demo-dark'].emit('change');
  assert.equal(sessions.length, 1);
  assert.equal(nodes['#scheme-label'].textContent, 'Light');
  assert.deepEqual(JSON.parse(JSON.stringify(sessions[0].messages)), [{ type: 'theme', payload: { dark: false, family: 'hybrid' } }]);
});

test('Theme choices made during warmup apply when the original worker becomes ready', async () => {
  const { nodes, themes, sessions } = await page();
  themes[1].emit('click');
  nodes['#demo-dark'].checked = false;
  nodes['#demo-dark'].emit('change');
  assert.equal(sessions.length, 1);
  sessions[0].ready = true;
  sessions[0].onStatus('ready', { backend: 'worker-webgpu' });
  assert.equal(sessions.length, 1);
  assert.deepEqual(JSON.parse(JSON.stringify(sessions[0].messages[0])), { type: 'theme', payload: { dark: false, family: 'restrained' } });
});


test('Native media pages retain the preview without starting a worker', async () => {
  const { nodes, sessions, tabs } = await page({ slug: 'audio-player', live: false });
  tabs[1].emit('click');
  assert.equal(sessions.length, 0);
  assert.equal(nodes['#live-canvas'].dataset.status, 'native');
  assert.equal(nodes['#live-canvas'].attributes['aria-busy'], 'false');
  assert.equal(nodes['.controls'].hidden, true);
  assert.equal(nodes['.css-editor'].hidden, true);
  assert.match(nodes['#live-status'].textContent, /Live playback is not available/);
});
