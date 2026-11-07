import { test } from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import { readFile } from 'node:fs/promises';

async function load() {
  const module = { exports: {} };
  const source = await readFile(new URL('../../../docs/book/theme/blinc-example.js', import.meta.url), 'utf8');
  vm.runInNewContext(source, { module });
  return module.exports;
}

const page = (worker, remap) => `<head><script>(function(){var m=${remap},b=document.baseURI,f=window.fetch,t={};window.blincAssetRemap=m;})();</script></head>
<script type="module">
  const wasmUrl = "pkg/strangler_demo_bg.wasm";
  const canWorker = ${worker}
    && !!HTMLCanvasElement.prototype.transferControlToOffscreen;
  async function startOnPage() { const { default: init } = await import("./pkg/strangler_demo.js"); await init(); }
</script>`;

test('The generated example page yields its module, wasm, remap and worker flag', async () => {
  const { parseExamplePage } = await load();
  const parsed = parseExamplePage(page('true', '{"examples/a.glb":"https://assets.blinc.rs/asset/x.glb"}'));
  assert.deepEqual(JSON.parse(JSON.stringify(parsed)), {
    js: 'strangler_demo', wasm: 'pkg/strangler_demo_bg.wasm', worker: true,
    assets: { 'examples/a.glb': 'https://assets.blinc.rs/asset/x.glb' },
  });
  assert.equal(parseExamplePage(page('false', '{}')).worker, false);
});

test('A page without the generated markers, or with a broken remap, is rejected', async () => {
  const { parseExamplePage } = await load();
  assert.equal(parseExamplePage('<html></html>'), null);
  assert.equal(parseExamplePage(page('true', '{oops}')), null);
});
