import assert from 'node:assert/strict';
import { readFile, access } from 'node:fs/promises';
import path from 'node:path';
import crypto from 'node:crypto';

const site = path.resolve(process.argv[2] || 'target/component-site');
const components = path.join(site, 'components');
const manifest = JSON.parse(await readFile(path.join(components, 'build-manifest.json'), 'utf8'));
assert.equal(manifest.schema_version, 1);
assert.equal(manifest.runtime_built, true, 'run the catalog build with --build before the artifact gate');
assert.equal(manifest.runtime_channel, 'unreleased');
assert.match(manifest.runtime_inputs_sha256, /^[0-9a-f]{64}$/);
assert.match(manifest.cargo_lock_sha256, /^[0-9a-f]{64}$/);
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');

for (const [relative, expected] of Object.entries(manifest.artifacts_sha256)) {
  assert.equal(sha(await readFile(path.join(components, relative))), expected, `checksum: ${relative}`);
}
const release = `releases/${manifest.runtime_inputs_sha256}`;
const module = await readFile(path.join(components, release, 'runtime/pkg/blinc_component_demos.js'), 'utf8');
assert.match(module, /create_runtime/);
assert.match(module, /warm_frame/);
const wasm = await readFile(path.join(components, release, 'runtime/pkg/blinc_component_demos_bg.wasm'));
assert.equal(wasm.subarray(0, 4).toString('hex'), '0061736d');
assert.equal(WebAssembly.validate(wasm), true, 'the built WASM module validates');

// Count actual GIF frames and their delays, including any local colour tables.
function gifInfo(bytes) {
  assert.match(bytes.toString('ascii', 0, 6), /^GIF8[79]a$/);
  let offset = 13 + ((bytes[10] & 0x80) ? 3 * (1 << ((bytes[10] & 7) + 1)) : 0);
  let frames = 0;
  const delays = [];
  const blocks = () => {
    while (bytes[offset] !== 0) { const length = bytes[offset++]; offset += length; }
    offset++;
  };
  while (offset < bytes.length) {
    const type = bytes[offset++];
    if (type === 0x3b) break;
    if (type === 0x21) {
      const label = bytes[offset++];
      if (label === 0xf9) delays.push(bytes.readUInt16LE(offset + 2) * 10);
      blocks();
    } else if (type === 0x2c) {
      const packed = bytes[offset + 8]; offset += 9;
      if (packed & 0x80) offset += 3 * (1 << ((packed & 7) + 1));
      offset++; blocks(); frames++;
    } else throw new Error(`Invalid GIF block ${type} at ${offset - 1}`);
  }
  return { width: bytes.readUInt16LE(6), height: bytes.readUInt16LE(8), frames, delays };
}

const pages = ['components/index.html', 'components/installation/index.html'];
assert.ok(manifest.routes.includes('/components/installation/'));
const installation = await readFile(path.join(components, 'installation/index.html'), 'utf8');
assert.ok(installation.includes('WindowedApp::run_with_theme'));
assert.equal(installation.includes('http-equiv="refresh"'), false);
assert.equal(installation.includes('blinc-book'), false);
const fixtures = manifest.components.flatMap(component => [
  { ...component, route: `/components/${component.library}/${component.slug}/` },
  ...(component.examples || []).map(example => ({ ...component, ...example, examples: [], route: `/components/${component.library}/${component.slug}/examples/${example.slug}/` })),
]);
for (const component of fixtures) {
  assert.ok(['cn', 'layout'].includes(component.library));
  const route = component.route;
  assert.ok(manifest.routes.includes(route));
  const relative = `${route.slice(1)}index.html`;
  pages.push(relative);
  const html = await readFile(path.join(site, relative), 'utf8');
  assert.equal((html.match(/id="live-canvas"/g) || []).length, 1);
  const config = JSON.parse(html.match(/<script type="application\/json" id="component-config">([^<]+)<\/script>/)[1]);
  assert.equal(config.slug, component.slug);
  assert.equal(config.build, manifest.runtime_inputs_sha256);
  assert.equal((html.match(/role="tab"/g) || []).length, 3 * (1 + (component.examples || []).length));
  assert.ok(html.includes(`/components/releases/${config.build}/detail.js`));
  assert.ok(html.includes(`pub fn ${component.entry}(`), 'Code contains the selected compiled fixture');
  const preview = manifest.previews[component.slug];
  const animated = component.capabilities.some(capability => ['pointer', 'keyboard', 'animation'].includes(capability));
  assert.equal(preview.animated, animated);
  assert.equal(config.animated, animated);
  assert.equal(preview.theme, 'hybrid');
  assert.equal(preview.scheme, 'dark');
  assert.match(html, /role="switch" id="demo-dark"[^>]*checked/);
  assert.equal((html.match(/data-theme-family=/g) || []).length, 3);
  assert.ok(html.includes('id="custom-css"'));
  assert.ok(html.includes('id="apply-css"'));
  assert.equal((html.match(/data-chart-section/g) || []).length, (component.examples || []).length);
  assert.ok(html.includes('language-rust'));
  assert.equal(html.includes('demo-viewport'), false);
  assert.equal(html.includes('CSS first'), false);
  if (animated) {
    const gif = gifInfo(await readFile(path.join(components, `previews/${component.slug}/preview.gif`)));
    assert.equal(gif.width, component.viewport.width * component.viewport.scale); assert.equal(gif.height, component.viewport.height * component.viewport.scale);
    assert.equal(gif.frames, preview.frames); assert.ok(gif.frames > 1);
    assert.equal(gif.delays.length, gif.frames);
    assert.ok(gif.delays.every(delay => delay === preview.delay_ms));
  } else {
    assert.equal(preview.frames, 1);
    assert.equal(preview.cursor, 'none');
    assert.equal(html.includes(`/previews/${component.slug}/preview.gif`), false);
    await assert.rejects(access(path.join(components, `previews/${component.slug}/preview.gif`)));
  }
  const png = await readFile(path.join(components, `previews/${component.slug}/poster.png`));
  assert.equal(png.readUInt32BE(16), component.viewport.width * component.viewport.scale); assert.equal(png.readUInt32BE(20), component.viewport.height * component.viewport.scale);
}
for (const relative of pages) {
  const html = await readFile(path.join(site, relative), 'utf8');
  assert.ok(html.includes('id="page-scheme"'));
  assert.equal(html.includes('{{'), false, `unexpanded template: ${relative}`);
  assert.equal(html.includes('CN components</h2>'), false);
  for (const match of html.matchAll(/(?:href|src|data-poster|data-animation)="(\/components\/[^"?#]*)(?:\?[^"#]*)?(?:#[^"]*)?"/g)) {
    let local = match[1].slice(1);
    if (local.endsWith('/')) local += 'index.html';
    await access(path.join(site, local)).catch(() => { throw new Error(`Broken catalog asset/link in ${relative}: ${local}`); });
  }
}
const index = await readFile(path.join(components, 'index.html'), 'utf8');
assert.equal(index.includes('detail.js'), false);
assert.equal(index.includes('.wasm'), false);
assert.equal((index.match(/data-component=/g) || []).length, manifest.components.length);
assert.equal((index.match(/data-grid-preview/g) || []).length, manifest.components.length);
assert.ok(index.includes('data-filter='));
assert.equal(index.includes('<select'), false);
for (const file of ['code.js', 'scheme.js', 'syntax/format.js', 'syntax/highlight.js', 'syntax/LICENSE']) await access(path.join(components, release, file));
console.log(`Validated ${manifest.components.length} component pages / ${fixtures.length} fixtures, GIF sequences, source, versioned WASM and ${Object.keys(manifest.artifacts_sha256).length} artifact checksums.`);
