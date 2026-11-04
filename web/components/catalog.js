const search = document.querySelector('#component-search');
const filters = [...document.querySelectorAll('[data-filter]')];
const cards = [...document.querySelectorAll('[data-component]')];
const previews = [...document.querySelectorAll('[data-grid-preview]')];
const toggle = document.querySelector('#toggle-grid-animation');
const reduced = matchMedia('(prefers-reduced-motion: reduce)');
const visible = new Set();
let category = '', playing = !reduced.matches;
function animate() {
  for (const image of previews) {
    const active = playing && !document.hidden && visible.has(image) && !image.closest('[data-component]').hidden;
    const source = active ? image.dataset.animation : image.dataset.poster;
    if (image.getAttribute('src') !== source) image.src = source;
  }
  if (toggle) {
    toggle.setAttribute('aria-pressed', String(playing));
    toggle.querySelector('.animation-label').textContent = playing ? 'Pause previews' : 'Play previews';
  }
}
function filter() {
  const words = (search?.value || '').trim().toLowerCase().split(/\s+/).filter(Boolean);
  let count = 0;
  for (const card of cards) {
    const show = words.every(word => card.dataset.search.includes(word)) && (!category || card.dataset.category === category);
    card.hidden = !show;
    if (show) count++;
  }
  document.querySelector('#result-count').textContent = `${count} component${count === 1 ? '' : 's'}`;
  document.querySelector('#empty-results').hidden = count > 0;
  animate();
}
for (const button of filters) button.addEventListener('click', () => {
  category = button.dataset.filter;
  for (const item of filters) item.setAttribute('aria-pressed', String(item === button));
  filter();
});
if (typeof IntersectionObserver !== 'undefined') {
  const observer = new IntersectionObserver(entries => {
    for (const entry of entries) {
      if (entry.isIntersecting) visible.add(entry.target); else visible.delete(entry.target);
    }
    animate();
  });
  for (const preview of previews) observer.observe(preview);
} else { for (const preview of previews) visible.add(preview); }
search?.addEventListener('input', filter);
toggle?.addEventListener('click', () => { playing = !playing; animate(); });
reduced.addEventListener('change', event => { playing = !event.matches; animate(); });
document.addEventListener('visibilitychange', animate);
document.addEventListener('keydown', event => {
  if (event.key === '/' && !event.ctrlKey && !event.metaKey && !['INPUT', 'TEXTAREA'].includes(document.activeElement?.tagName)) {
    event.preventDefault(); search?.focus();
  }
});
animate();
