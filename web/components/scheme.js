const root = document.documentElement;
const toggle = document.querySelector('#page-scheme');
const apply = scheme => {
  root.dataset.scheme = scheme;
  if (toggle) toggle.checked = scheme === 'dark';
  document.querySelector('meta[name="theme-color"]')?.setAttribute('content', scheme === 'dark' ? '#0b0e16' : '#f6e9eb');
};
apply(root.dataset.scheme === 'light' ? 'light' : 'dark');
toggle?.addEventListener('change', () => {
  const scheme = toggle.checked ? 'dark' : 'light';
  apply(scheme);
  try { localStorage.setItem('blinc-scheme', scheme); } catch {}
  document.dispatchEvent(new CustomEvent('page-scheme-change', { detail: scheme }));
});
window.addEventListener('storage', event => {
  if (event.key !== 'blinc-scheme' || !['light', 'dark'].includes(event.newValue)) return;
  apply(event.newValue);
  document.dispatchEvent(new CustomEvent('page-scheme-change', { detail: event.newValue }));
});
