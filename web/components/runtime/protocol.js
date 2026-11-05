export const VERSION = 1;
export const HOST_TYPES = new Set(['boot', 'input', 'resize', 'theme', 'props', 'css', 'pause', 'resume', 'dispose', 'media-state', 'media-frame']);
export const WORKER_TYPES = new Set(['progress', 'first-frame', 'ready', 'state', 'css-result', 'error', 'disposed', 'media']);

export function envelope(instance, seq, type, payload = {}) {
  return { version: VERSION, instance, seq, type, payload };
}

export function valid(message, instance, types) {
  return message?.version === VERSION && message.instance === instance
    && Number.isSafeInteger(message.seq) && message.seq >= 0
    && types.has(message.type) && message.payload !== null
    && typeof message.payload === 'object' && !Array.isArray(message.payload);
}
