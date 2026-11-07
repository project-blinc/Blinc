import { MediaHost } from './media-host.js';
import { WORKER_TYPES, envelope, valid } from './protocol.js';

/** Own exactly one transferable canvas and worker per component page. */
export class LiveDemo {
  constructor(container, options, onStatus) {
    this.container = container;
    this.options = options;
    this.onStatus = onStatus;
    this.seq = 0;
    this.lastSeq = -1;
    this.ready = false;
    this.visible = false;
    this.state = {};
    this.instance = crypto.randomUUID();
    this.listeners = new AbortController();
    this.media = new MediaHost((...args) => this.send(...args), message => this.onStatus('media-error', { message }));
    this.media.video = options.slug === 'video-player';
    this.start();
  }
  send(type, payload = {}, transfer = []) {
    this.worker?.postMessage(envelope(this.instance, this.seq++, type, payload), transfer);
  }
  start() {
    if (!HTMLCanvasElement.prototype.transferControlToOffscreen || typeof Worker === 'undefined') {
      this.onStatus('error', { message: 'Worker canvas rendering is unavailable in this browser. Preview and source are still available.' });
      return;
    }
    try {
      this.canvas = document.createElement('canvas');
      this.canvas.setAttribute('aria-label', `${this.options.title} live Blinc demo`);
      this.canvas.tabIndex = 0;
      this.editor = document.createElement('textarea');
      this.editor.className = 'demo-input-bridge';
      this.editor.setAttribute('aria-label', `${this.options.title} demo keyboard input`);
      this.editor.setAttribute('autocapitalize', 'off');
      this.editor.autocomplete = 'off'; this.editor.spellcheck = false;
      this.editor.tabIndex = -1;
      // The keyboard is raised by a tap and dropped only once the demo says nothing is being edited.
      this.editor.inputMode = 'none';
      this.editable = false;
      this.container.replaceChildren(this.canvas, this.editor);
      this.bindInput();
      const rect = this.container.getBoundingClientRect();
      const width = Math.max(1, rect.width || this.options.width), height = Math.max(1, rect.height || this.options.height);
      const canvas = this.canvas.transferControlToOffscreen();
      this.worker = new Worker(new URL(`./worker.js?v=${encodeURIComponent(this.options.build)}`, import.meta.url), { type: 'module' });
      this.worker.onmessage = ({ data }) => {
        if (this.stopped || !valid(data, this.instance, WORKER_TYPES) || data.seq <= this.lastSeq) return;
        this.lastSeq = data.seq;
        if (data.type === 'media') { this.media.request(data.payload); return; }
        if (data.type === 'ready') { this.ready = true; clearTimeout(this.timeout); this.setVisible(this.visible); }
        if (data.type === 'ready' || data.type === 'state') {
          this.state = data.payload;
          this.canvas.style.cursor = data.payload.cursor || 'default';
          // iOS drops a keyboard whose input mode is switched off while the field is focused and does
          // not raise it again outside a tap, so a report never turns the keyboard off by itself:
          // `settleKeyboard` does, after a tap, once the demo has had time to take focus.
          this.editable = !!data.payload.editable;
          if (this.editable) this.editor.inputMode = 'text';
        }
        if (data.type === 'error') this.stop();
        this.onStatus(data.type, data.payload);
      };
      this.worker.onerror = event => { this.stop(); this.onStatus('error', { message: event.message || 'Worker startup failed.' }); };
      this.send('boot', { ...this.options, canvas, width, height, dpr: devicePixelRatio || 1 }, [canvas]);
      this.resizeObserver = new ResizeObserver(() => this.resize());
      this.resizeObserver.observe(this.container);
      this.resizeListener = () => this.resize();
      window.addEventListener('resize', this.resizeListener, { signal: this.listeners.signal });
      // Resolution media queries fire when a window moves between different-DPR displays.
      this.watchDpr();
      this.timeout = setTimeout(() => {
        this.stop(); this.onStatus('error', { message: 'The worker did not finish loading. Try again, or use Preview and Code.' });
      }, 90000);
    } catch (error) { this.stop(); this.onStatus('error', { message: String(error.message || error) }); }
  }
  watchDpr() {
    this.dprQuery?.removeEventListener('change', this.dprListener);
    this.dprQuery = matchMedia(`(resolution: ${devicePixelRatio}dppx)`);
    this.dprListener = () => { this.resize(); this.watchDpr(); };
    this.dprQuery.addEventListener('change', this.dprListener, { once: true });
  }
  resize() {
    if (!this.ready) return;
    const rect = this.container.getBoundingClientRect();
    if (rect.width > 0 && rect.height > 0) this.send('resize', { width: rect.width, height: rect.height, dpr: devicePixelRatio || 1 });
  }
  setVisible(visible) {
    this.visible = visible;
    if (!visible || document.hidden) this.media.pause();
    this.send(visible && !document.hidden ? 'resume' : 'pause');
    if (this.ready && visible) this.resize();
  }
  input(event, source = {}) {
    if (!this.ready || !this.visible) return;
    const timestamp = Number.isFinite(source.timeStamp) ? source.timeStamp : performance.now();
    this.send('input', { event, timestamp, modifiers: {
      shift: source.shiftKey, ctrl: source.ctrlKey, alt: source.altKey, meta: source.metaKey,
    }});
  }
  bindInput() {
    const listen = (target, event, fn, options = {}) => target.addEventListener(event, fn, { ...options, signal: this.listeners.signal });
    const position = event => {
      const rect = this.canvas.getBoundingClientRect();
      return { x: event.clientX - rect.left, y: event.clientY - rect.top };
    };
    listen(this.container, 'contextmenu', e => e.preventDefault());
    listen(this.canvas, 'dragstart', e => e.preventDefault());
    listen(this.canvas, 'selectstart', e => e.preventDefault());
    // A finger has no wheel: its drag is sent as scroll steps, and its lift ends the gesture so a
    // flick carries on and an overscroll springs back. The browser's own panning is off on the canvas.
    const endTouch = e => { if (this.touch) { this.touch = null; this.input({ kind: 'touch-end' }, e); } };
    listen(this.canvas, 'pointermove', e => {
      const at = position(e);
      this.input({ kind: 'pointer-move', ...at }, e);
      if (this.touch && e.pointerType === 'touch') {
        const dx = at.x - this.touch.x, dy = at.y - this.touch.y;
        this.touch = at;
        if (dx || dy) this.input({ kind: 'touch-scroll', dx, dy }, e);
      }
    });
    listen(this.canvas, 'pointerdown', e => {
      e.preventDefault(); this.editor.focus({ preventScroll: true });
      this.pointerDown = true;
      this.downAt = position(e);
      this.canvas.setPointerCapture(e.pointerId);
      if (e.pointerType === 'touch') { this.touch = position(e); this.input({ kind: 'touch-start' }, e); }
      this.input({ kind: 'pointer-move', ...position(e) }, e);
      this.input({ kind: 'pointer-down', ...position(e), button: e.button }, e);
    });
    listen(this.canvas, 'pointerup', e => {
      this.pointerDown = false;
      this.input({ kind: 'pointer-up', ...position(e), button: e.button }, e);
      endTouch(e);
      // A drag scrolls; only a tap can be an edit, so only a tap raises the keyboard.
      const at = position(e);
      if (!this.downAt || Math.hypot(at.x - this.downAt.x, at.y - this.downAt.y) <= 10) { this.armKeyboard(); this.settleKeyboard(); }
      if (this.canvas.hasPointerCapture(e.pointerId)) this.canvas.releasePointerCapture(e.pointerId);
    });
    for (const event of ['pointerleave', 'pointercancel']) {
      listen(this.canvas, event, e => { this.input({ kind: 'pointer-leave' }); endTouch(e); });
    }
    listen(this.canvas, 'lostpointercapture', e => {
      if (this.pointerDown) {
        this.pointerDown = false;
        this.input({ kind: 'pointer-leave' });
      }
      endTouch(e);
    });
    listen(this.canvas, 'wheel', e => {
      e.preventDefault();
      const unit = e.deltaMode === WheelEvent.DOM_DELTA_LINE ? 16 : e.deltaMode === WheelEvent.DOM_DELTA_PAGE ? this.container.clientHeight : 1;
      this.input({ kind: 'pointer-move', ...position(e) }, e);
      this.input({ kind: 'wheel', x: -e.deltaX * unit, y: -e.deltaY * unit }, e);
    }, { passive: false });
    listen(this.canvas, 'focus', () => this.editor.focus({ preventScroll: true }));
    listen(this.editor, 'focus', () => this.input({ kind: 'focus', focused: true }));
    listen(this.editor, 'blur', () => this.input({ kind: 'focus', focused: false }));
    let composing = false;
    listen(this.editor, 'compositionstart', () => { composing = true; });
    listen(this.editor, 'compositionend', e => {
      composing = false;
      if (e.data) this.input({ kind: 'text', text: e.data });
      this.editor.value = '';
    });
    listen(this.editor, 'input', e => {
      if (composing || e.isComposing) return;
      // Composition text is committed by compositionend, once.
      if (e.inputType !== 'insertCompositionText' && e.inputType !== 'insertFromComposition' && e.inputType !== 'insertFromPaste' && e.data) {
        this.input({ kind: 'text', text: e.data });
      }
      this.editor.value = '';
    });
    listen(this.editor, 'keydown', e => {
      if (e.isComposing) return;
      const code = e.keyCode || e.which;
      if (e.key === 'Escape') {
        e.preventDefault(); this.input({ kind: 'key-down', code }, e);
        this.container.dispatchEvent(new CustomEvent('demo-exit', { bubbles: true }));
        return;
      }
      if (['Tab', 'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'Home', 'End', 'PageUp', 'PageDown'].includes(e.key)) e.preventDefault();
      // Clipboard events retain browser user activation and handle these chords.
      if ((e.ctrlKey || e.metaKey) && ['c', 'x', 'v'].includes(e.key.toLowerCase())) return;
      this.input({ kind: 'key-down', code }, e);
    });
    listen(this.editor, 'keyup', e => this.input({ kind: 'key-up', code: e.keyCode || e.which }, e));
    listen(this.editor, 'paste', e => {
      const text = e.clipboardData?.getData('text/plain');
      if (text) { e.preventDefault(); this.input({ kind: 'text', text }); }
    });
    for (const type of ['copy', 'cut']) listen(this.editor, type, e => {
      if (this.state.selection && e.clipboardData) {
        e.preventDefault(); e.clipboardData.setData('text/plain', this.state.selection);
        if (type === 'cut') this.input({ kind: 'key-down', code: 8 });
      }
    });
  }
  /** Raise the keyboard now, inside the tap iOS requires, and drop it later if nothing took focus. */
  armKeyboard() {
    clearTimeout(this.keyboardTimer);
    this.editor.inputMode = 'text';
    this.editor.focus({ preventScroll: true });
  }
  settleKeyboard() {
    clearTimeout(this.keyboardTimer);
    this.keyboardTimer = setTimeout(() => { if (!this.editable) this.editor.inputMode = 'none'; }, 400);
  }
  stop() {
    if (this.stopped) return;
    clearTimeout(this.keyboardTimer);
    this.stopped = true;
    clearTimeout(this.timeout);
    this.ready = false;
    this.resizeObserver?.disconnect();
    this.dprQuery?.removeEventListener('change', this.dprListener);
    this.listeners.abort();
    this.media.stop();
    this.send('dispose');
    if (this.worker) { this.worker.onmessage = null; this.worker.onerror = null; this.worker.terminate(); }
    this.worker = null;
  }
}
