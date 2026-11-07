import { VERSION, HOST_TYPES, envelope, valid } from './protocol.js';

let instance, runtime, seq = 0, lastInput = -1, active = false, frameId = null;
let booted = false, disposed = false, failed = false, lastState = '', syncToken = null;
const emit = (type, payload) => postMessage(envelope(instance, seq++, type, payload));
function cancelFrame() {
  if (frameId !== null) cancelAnimationFrame(frameId);
  frameId = null;
}
function fail(error) {
  if (failed || disposed) return;
  failed = true;
  active = false;
  cancelFrame();
  emit('error', { message: String(error?.message || error) });
}
function draw() {
  frameId = null;
  if (!runtime || disposed || failed) return;
  try {
    const state = runtime.frame();
    const parsed = JSON.parse(state);
    // Selection and cursor are host services; only send changes to their values.
    const key = JSON.stringify({ ...parsed, frames: undefined });
    // A sync request is answered by the next frame, changed or not, so the host knows the report
    // follows every input it sent before asking.
    if (syncToken !== null) { emit('state', { ...parsed, sync: syncToken }); syncToken = null; lastState = key; }
    else if (key !== lastState) { lastState = key; emit('state', parsed); }
    if (active) frameId = requestAnimationFrame(draw);
  } catch (error) { fail(error); }
}

// The page's asset remap, applied to the worker's own fetch: the shim on the page cannot reach it.
function remapAssets(map) {
  const table = new Map(Object.entries(map || {}));
  if (!table.size) return;
  const native = self.fetch.bind(self);
  self.fetch = (input, init) => {
    const direct = typeof input === 'string' || input instanceof URL;
    const target = table.get(direct ? String(input) : input?.url);
    return native(target ? (direct ? target : new Request(target, input)) : input, init);
  };
}

const media = new Map();
function mediaBridge(id, operation, value) {
  const state = media.get(id) || { position: 0, duration: 0, buffered: 0, paused: true, ended: false };
  if (operation === 'frame') {
    const frame = state.frame; state.frame = null;
    if (frame) emit('media', { id, operation: 'frame-consumed' });
    return frame || null;
  }
  if (['position', 'duration', 'buffered', 'paused', 'ended'].includes(operation)) return state[operation];
  if (operation === 'create') media.set(id, state);
  emit('media', { id, operation, value });
}
self.onmessage = async ({ data }) => {
  if (!booted) {
    if (data?.version !== VERSION || data.type !== 'boot' || typeof data.instance !== 'string') return;
    instance = data.instance;
    booted = true;
  } else if (data.type === 'boot') return;
  if (!valid(data, instance, HOST_TYPES) || data.seq <= lastInput || disposed || failed) return;
  lastInput = data.seq;
  const payload = data.payload;
  try {
    switch (data.type) {
      case 'boot': {
        emit('progress', { message: 'Loading the Blinc runtime…' });
        let wasm, created;
        if (payload.kind === 'example') {
          // An example is its own module; the page supplies where it and its assets live.
          remapAssets(payload.assets);
          wasm = await import(payload.module);
          await wasm.default({ module_or_path: payload.wasm });
          emit('progress', { message: 'Creating a worker GPU surface…' });
          created = await wasm.create_runtime(payload.canvas, payload.width, payload.height, payload.dpr,
            !!payload.dark, payload.base);
        } else {
          const moduleUrl = new URL('./pkg/blinc_component_demos.js', import.meta.url);
          moduleUrl.searchParams.set('v', payload.build);
          wasm = await import(moduleUrl.href);
          await wasm.default({ module_or_path: new URL(`./pkg/blinc_component_demos_bg.wasm?v=${encodeURIComponent(payload.build)}`, import.meta.url) });
          wasm.set_media_bridge(mediaBridge);
          emit('progress', { message: 'Creating a worker GPU surface…' });
          created = await wasm.create_runtime(payload.canvas, payload.width, payload.height, payload.dpr,
            payload.slug, payload.dark, payload.family, payload.css || '');
        }
        if (disposed) { created.free(); return; }
        if (payload.kind !== 'example') created.control(payload.disabled, payload.progress);
        // Warm a real, nonzero surface once even while Preview is selected.
        const state = JSON.parse(await created.warm_frame());
        if (disposed) { created.free(); return; }
        runtime = created;
        emit('first-frame', state);
        emit('ready', state);
        if (active && frameId === null) frameId = requestAnimationFrame(draw);
        break;
      }
      case 'resume': active = true; if (runtime && frameId === null) frameId = requestAnimationFrame(draw); break;
      case 'pause': active = false; cancelFrame(); break;
      case 'dispose':
        active = false; disposed = true; cancelFrame(); runtime?.free(); runtime = null;
        emit('disposed', {}); self.close(); break;
      case 'media-state':
        Object.assign(media.get(payload.id) || {}, payload);
        if (runtime && !active && frameId === null) frameId = requestAnimationFrame(draw);
        break;
      case 'media-frame': {
        const state = media.get(payload.id);
        if (state) state.frame = { width: payload.width, height: payload.height, pixels: payload.pixels };
        if (runtime && !active && frameId === null) frameId = requestAnimationFrame(draw);
        break;
      }
      case 'input':
        if (runtime) {
          const mods = payload.modifiers || {};
          runtime.input(JSON.stringify(payload.event), !!mods.shift, !!mods.ctrl, !!mods.alt, !!mods.meta, payload.timestamp);
        }
        break;
      case 'sync': syncToken = payload.token; if (runtime && frameId === null) frameId = requestAnimationFrame(draw); break;
      case 'resize': if (runtime) runtime.resize(payload.width, payload.height, payload.dpr); break;
      case 'theme': if (runtime) runtime.theme(payload.dark, payload.family); break;
      case 'props': if (runtime) runtime.control(payload.disabled, payload.progress); break;
      case 'css':
        if (runtime) {
          try {
            runtime.apply_css(payload.css);
            emit('css-result', { request: payload.request, ok: true });
            if (!active && frameId === null) frameId = requestAnimationFrame(draw);
          } catch (error) {
            emit('css-result', { request: payload.request, ok: false, message: String(error?.message || error) });
          }
        }
        break;
    }
  } catch (error) { fail(error); }
};
self.addEventListener('unhandledrejection', event => { event.preventDefault(); fail(event.reason); });
