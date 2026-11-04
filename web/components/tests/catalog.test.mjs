import { test } from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import { readFile } from 'node:fs/promises';
import { highlight, numbered } from '../syntax/format.js';

test('Grid previews animate while visible and stop for filtering, pause and reduced motion', async () => {
  class Element {
    constructor(data = {}) { this.dataset = data; this.events = {}; this.attributes = {}; this.hidden = false; this.value = ''; }
    addEventListener(type, callback) { this.events[type] = callback; }
    emit(type, event = {}) { this.events[type]?.(event); }
    setAttribute(name, value) { this.attributes[name] = value; }
    getAttribute(name) { return name === 'src' ? this.src : this.attributes[name]; }
  }
  const cards = [new Element({ category: 'Actions', search: 'button actions' }), new Element({ category: 'Forms', search: 'input forms' })];
  const previews = cards.map((card, index) => Object.assign(new Element({ poster: `${index}.png`, animation: `${index}.gif` }), { closest: () => card }));
  const filters = ['', 'Actions', 'Forms'].map(filter => new Element({ filter }));
  const search = new Element(), toggle = new Element(), label = new Element(), count = new Element(), empty = new Element();
  toggle.querySelector = () => label;
  const reduced = Object.assign(new Element(), { matches: false });
  let intersection;
  const document = Object.assign(new Element(), {
    querySelector: selector => ({ '#component-search': search, '#toggle-grid-animation': toggle, '#result-count': count, '#empty-results': empty })[selector],
    querySelectorAll: selector => ({ '[data-filter]': filters, '[data-component]': cards, '[data-grid-preview]': previews })[selector],
  });
  const context = vm.createContext({ document, matchMedia: () => reduced, IntersectionObserver: class { constructor(callback) { intersection = callback; } observe() {} } });
  new vm.Script(await readFile(new URL('../catalog.js', import.meta.url), 'utf8')).runInContext(context);
  assert.deepEqual(previews.map(image => image.src), ['0.png', '1.png']);
  intersection(previews.map(target => ({ target, isIntersecting: true })));
  assert.deepEqual(previews.map(image => image.src), ['0.gif', '1.gif']);
  filters[1].emit('click');
  assert.deepEqual(previews.map(image => image.src), ['0.gif', '1.png']);
  assert.equal(count.textContent, '1 component');
  toggle.emit('click');
  assert.equal(previews[0].src, '0.png');
  toggle.emit('click');
  assert.equal(previews[0].src, '0.gif');
  document.hidden = true; document.emit('visibilitychange');
  assert.equal(previews[0].src, '0.png');
  document.hidden = false; document.emit('visibilitychange');
  reduced.emit('change', { matches: true });
  assert.equal(previews[0].src, '0.png');
  search.value = 'missing'; search.emit('input');
  assert.equal(empty.hidden, false);
  assert.equal(count.textContent, '0 components');
});

test('Bundled syntax highlighting supports the compiled Rust and Blinc CSS sources', () => {
  const rust = 'pub fn button_demo() { Button::new().label("<Save>"); }';
  const css = '.cn-button:hover { background: var(--primary); }';
  const rustHtml = highlight(rust, 'rust');
  const cssHtml = highlight(css, 'css');
  assert.match(rustHtml, /hljs-keyword/);
  assert.match(rustHtml, /&lt;Save&gt;/);
  assert.match(cssHtml, /hljs-selector-class/);
  assert.match(cssHtml, /hljs-attribute/);
  assert.match(rustHtml, /hljs-type">Button/);
  assert.match(rustHtml, /hljs-call">label/);
  const multiline = '/* first\nsecond */\n.x { color: red; }';
  assert.equal(numbered(multiline, 'css').replace(/<[^>]*>/g, ''), multiline);
});
