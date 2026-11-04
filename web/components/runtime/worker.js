import { VERSION, HOST_TYPES, envelope, valid } from './protocol.js';

let instance, runtime, seq = 0, lastInput = -1, active = false, frameId = null;
let booted = false, disposed = false, failed = false, lastState = '';
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
    if (key !== lastState) { lastState = key; emit('state', parsed); }
    if (active) frameId = requestAnimationFrame(draw);
  } catch (error) { fail(error); }
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
        const moduleUrl = new URL('./pkg/blinc_component_demos.js', import.meta.url);
        moduleUrl.searchParams.set('v', payload.build);
        const wasm = await import(moduleUrl.href);
        await wasm.default({ module_or_path: new URL(`./pkg/blinc_component_demos_bg.wasm?v=${encodeURIComponent(payload.build)}`, import.meta.url) });
        emit('progress', { message: 'Creating a worker GPU surface…' });
        const created = await wasm.create_runtime(payload.canvas, payload.width, payload.height, payload.dpr,
          payload.slug, payload.dark, payload.family, payload.css || '');
        if (disposed) { created.free(); return; }
        created.control(payload.disabled, payload.progress);
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
      case 'input':
        if (runtime) {
          const mods = payload.modifiers || {};
          runtime.input(JSON.stringify(payload.event), !!mods.shift, !!mods.ctrl, !!mods.alt, !!mods.meta);
        }
        break;
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
