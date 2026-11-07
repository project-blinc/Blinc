// Runs a gallery example inside its book page: the page's `.blinc-example` placeholder gets the
// same worker host the component catalog uses, started when the placeholder scrolls into view.
(function () {
  'use strict';

  // The facts the generated example page holds: its module name, wasm location, asset remap and
  // whether a worker may run it.
  function parseExamplePage(html) {
    const js = /import\("\.\/pkg\/([^"]+)\.js"\)/.exec(html);
    const wasm = /const wasmUrl = "([^"]+)"/.exec(html);
    const worker = /const canWorker = (true|false)/.exec(html);
    const remap = /var m=([\s\S]*?),b=document\.baseURI/.exec(html);
    if (!js || !wasm || !worker) return null;
    let assets = {};
    if (remap) { try { assets = JSON.parse(remap[1]); } catch (error) { return null; } }
    return { js: js[1], wasm: wasm[1], worker: worker[1] === 'true', assets };
  }

  const api = { parseExamplePage };
  if (typeof window === 'undefined') { module.exports = api; return; }
  window.blincExample = api;

  const nodes = [...document.querySelectorAll('.blinc-example[data-example]')];
  if (!nodes.length) return;
  const root = typeof path_to_root === 'string' ? path_to_root : '';
  const live = new Set();
  const MAX_LIVE = 4;

  function show(node, text, link) {
    let status = node.querySelector('.blinc-example-status');
    if (!status) { status = document.createElement('div'); status.className = 'blinc-example-status'; node.append(status); }
    status.textContent = text;
    if (link) {
      const open = document.createElement('a');
      open.href = link; open.textContent = ' Open the example in its own page.';
      status.append(open);
    }
    return status;
  }

  async function mount(node, state) {
    const name = node.dataset.example;
    const dir = new URL(`${root}examples/${name}/`, location.href);
    const open = new URL('index.html', dir).href;
    const status = show(node, 'Loading demo…');
    try {
      const response = await fetch(open);
      if (!response.ok) throw new Error('The example page is missing.');
      const page = parseExamplePage(await response.text());
      if (!page) throw new Error('The example page has an unknown shape.');
      if (!page.worker || !HTMLCanvasElement.prototype.transferControlToOffscreen || typeof Worker === 'undefined') {
        throw new Error('This browser cannot run the demo in the page.');
      }
      const { LiveDemo } = await import(new URL(`${root}examples/_runtime/host.js`, location.href).href);
      const assets = Object.fromEntries(Object.entries(page.assets).map(([path, url]) => [new URL(path, open).href, url]));
      const rect = node.getBoundingClientRect();
      const light = ['light', 'rust'].some(theme => document.documentElement.classList.contains(theme));
      const demo = new LiveDemo(node, {
        kind: 'example', title: node.dataset.title || name, build: '1',
        module: new URL(`pkg/${page.js}.js`, dir).href, wasm: new URL(page.wasm, dir).href,
        base: open, assets, dark: !light, width: rect.width, height: rect.height,
      }, (type, payload) => {
        if (type === 'progress') show(node, payload.message || 'Loading demo…');
        if (type === 'ready') status.remove();
        if (type === 'error') { demo.stop(); live.delete(state); show(node, payload.message || 'The demo could not start.', open); }
      });
      state.demo = demo;
      node.append(status); // the host replaced the node's children
      demo.setVisible(state.visible);
    } catch (error) {
      live.delete(state);
      show(node, String(error.message || error), open);
    }
  }

  // Each demo owns a GPU context, so only a few stay alive: the oldest one off screen is released.
  function release() {
    for (const state of live) {
      if (live.size <= MAX_LIVE) return;
      if (state.visible || !state.demo) continue;
      state.demo.stop(); state.node.replaceChildren(); live.delete(state); state.started = false;
    }
  }

  const observer = new IntersectionObserver(entries => {
    for (const entry of entries) {
      const state = entry.target.__blinc;
      state.visible = entry.isIntersecting;
      if (state.demo) state.demo.setVisible(state.visible);
      if (state.visible && !state.started) {
        state.started = true; live.add(state); mount(state.node, state); release();
      }
    }
  }, { rootMargin: '200px' });
  for (const node of nodes) { node.__blinc = { node, visible: false, started: false }; observer.observe(node); }
  window.addEventListener('pagehide', () => { for (const state of live) state.demo?.stop(); });
})();
