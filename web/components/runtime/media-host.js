/** DOM decoding stays in the browser; Blinc controls and rendering stay in the worker. */
export class MediaHost {
  constructor(send, onError) { this.send = send; this.onError = onError; this.players = new Map(); }
  request({ id, operation, value }) {
    let p = this.players.get(id);
    if (operation === 'create') {
      if (p) return;
      const media = document.createElement('video');
      media.playsInline = true; media.preload = 'metadata'; media.hidden = true;
      media.crossOrigin = 'anonymous';
      document.body.append(media);
      const canvas = document.createElement('canvas');
      p = { media, canvas, context: canvas.getContext('2d', { willReadFrequently: true }), inFlight: false, disposed: false, clockFrame: null };
      this.players.set(id, p);
      const state = () => {
        let buffered = 0;
        for (let i = 0; i < media.buffered.length; i++) {
          buffered = media.buffered.end(i);
          if (media.currentTime >= media.buffered.start(i) && media.currentTime <= buffered) break;
        }
        this.send('media-state', { id, position: media.currentTime * 1000, duration: Number.isFinite(media.duration) ? media.duration * 1000 : 0,
          buffered: buffered * 1000, paused: media.paused, ended: media.ended });
      };
      const capture = () => {
        state();
        if (p.inFlight || p.disposed || !media.videoWidth || media.readyState < 2) return;
        const width = Math.min(840, media.videoWidth), height = Math.round(width * media.videoHeight / media.videoWidth);
        if (canvas.width !== width || canvas.height !== height) { canvas.width = width; canvas.height = height; }
        p.context.drawImage(media, 0, 0, width, height);
        const pixels = p.context.getImageData(0, 0, width, height).data.buffer;
        p.inFlight = true;
        this.send('media-frame', { id, width, height, pixels }, [pixels]);
      };
      for (const type of ['loadedmetadata', 'durationchange', 'play', 'pause', 'ended', 'timeupdate', 'progress']) media.addEventListener(type, state);
      if (!this.video) {
        const stopClock = () => { if (p.clockFrame !== null) cancelAnimationFrame(p.clockFrame); p.clockFrame = null; };
        const tick = () => {
          p.clockFrame = null;
          if (p.disposed || media.paused || media.ended) return;
          state();
          p.clockFrame = requestAnimationFrame(tick);
        };
        media.addEventListener('play', () => { if (p.clockFrame === null) p.clockFrame = requestAnimationFrame(tick); });
        for (const type of ['pause', 'ended']) media.addEventListener(type, stopClock);
      }
      for (const type of ['loadeddata', 'seeked']) media.addEventListener(type, capture);
      media.addEventListener('error', () => this.onError(media.error?.message || 'Media could not be loaded.'));
      const frame = () => {
        if (p.disposed) return;
        capture();
        p.frame = media.requestVideoFrameCallback(frame);
      };
      p.frame = media.requestVideoFrameCallback(frame);
      return;
    }
    if (!p) return;
    const media = p.media;
    switch (operation) {
      case 'load-url': media.src = value; media.load(); break;
      case 'load-bytes':
        if (p.url) URL.revokeObjectURL(p.url);
        p.url = URL.createObjectURL(new Blob([value])); media.src = p.url; media.load(); break;
      case 'play': media.play().catch(error => { if (error.name !== 'AbortError') this.onError(error.message); }); break;
      case 'pause': media.pause(); break;
      case 'seek': media.currentTime = Math.max(0, value / 1000); break;
      case 'volume': media.volume = Math.max(0, Math.min(1, value)); break;
      case 'frame-consumed': p.inFlight = false; break;
    }
  }
  pause() { for (const { media } of this.players.values()) media.pause(); }
  stop() {
    for (const p of this.players.values()) {
      p.disposed = true; p.media.pause(); p.media.cancelVideoFrameCallback(p.frame);
      if (p.clockFrame !== null) cancelAnimationFrame(p.clockFrame);
      p.media.removeAttribute('src'); p.media.load(); p.media.remove();
      if (p.url) URL.revokeObjectURL(p.url);
    }
    this.players.clear();
  }
}
