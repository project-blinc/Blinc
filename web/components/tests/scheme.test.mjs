import { test } from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import { readFile } from 'node:fs/promises';

test('Page colour scheme stays in sync with its switch and persists across pages', async () => {
  const root = { dataset: { scheme: 'light' } }, saved = [], events = [];
  let change;
  const toggle = { addEventListener(type, callback) { change = callback; } };
  const meta = { setAttribute(name, value) { this[name] = value; } };
  const context = vm.createContext({
    document: { documentElement: root, querySelector: selector => selector === '#page-scheme' ? toggle : meta, dispatchEvent: event => events.push(event) },
    localStorage: { setItem: (...entry) => saved.push(entry) },
    CustomEvent: class { constructor(type, options) { this.type = type; this.detail = options.detail; } },
    window: { addEventListener() {} },
  });
  new vm.Script(await readFile(new URL('../scheme.js', import.meta.url), 'utf8')).runInContext(context);
  assert.equal(toggle.checked, false);
  toggle.checked = true; change();
  assert.equal(root.dataset.scheme, 'dark');
  assert.deepEqual(saved, [['blinc-scheme', 'dark']]);
  assert.equal(events.at(-1).detail, 'dark');
  toggle.checked = false; change();
  assert.equal(root.dataset.scheme, 'light');
  assert.equal(events.at(-1).detail, 'light');
});
