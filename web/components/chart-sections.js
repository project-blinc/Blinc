import { LiveDemo } from './runtime/host.js';

const sections = [...document.querySelectorAll('[data-chart-section]')];
if (sections.length) {
  const reduced = matchMedia('(prefers-reduced-motion: reduce)');
  const visible = new Set();
  let active;
  const pageDark = () => document.documentElement.dataset.scheme !== 'light';
  const notify = () => active?.demo.setVisible(!document.hidden && visible.has(active.section) && !active.section.querySelector('[data-chart-panel="live"]').hidden);
  function closeActive() {
    if (!active) return;
    active.demo.stop();
    active.host.replaceChildren();
    active.select('preview');
    active = undefined;
  }
  document.addEventListener('page-scheme-change', event => {
    if (active?.demo.ready) active.demo.send('theme', { dark: event.detail === 'dark', family: 'hybrid' });
  });
  const primary = document.querySelector('.example');
  primary.querySelector('[data-tab="live"]').addEventListener('click', closeActive);
  for (const section of sections) {
    const buttons = [...section.querySelectorAll('[data-chart-tab]')];
    const panels = [...section.querySelectorAll('[data-chart-panel]')];
    const image = section.querySelector('[data-chart-image]');
    const toggle = section.querySelector('[data-chart-animation]');
    let tab = 'preview', playing = !reduced.matches;
    function animate() {
      const source = tab === 'preview' && playing && visible.has(section) && !document.hidden ? image.dataset.animation : image.dataset.poster;
      if (image.getAttribute('src') !== source) image.src = source;
      toggle.textContent = playing ? 'Pause preview' : 'Play preview';
      toggle.setAttribute('aria-pressed', String(playing));
    }
    function select(next, focus = false) {
      tab = next;
      for (const button of buttons) {
        const selected = button.dataset.chartTab === tab;
        button.setAttribute('aria-selected', String(selected));
        button.tabIndex = selected ? 0 : -1;
        if (selected && focus) button.focus();
      }
      for (const panel of panels) panel.hidden = panel.dataset.chartPanel !== tab;
      animate();
      if (tab !== 'live') { if (active?.section === section) notify(); return; }
      if (active?.section !== section) {
        closeActive();
        primary.querySelector('[data-tab="preview"]').click();
        const host = section.querySelector('[data-chart-live]');
        const status = document.createElement('div');
        status.className = 'chart-live-status';
        status.textContent = 'Loading live preview…';
        const demo = new LiveDemo(host, {
          ...JSON.parse(host.dataset.demoConfig), dark: pageDark(), family: 'hybrid',
        }, (type, data) => {
          if (type === 'ready') { status.remove(); notify(); }
          if (type === 'error') { status.textContent = 'Live preview is unavailable. Use Open to run it on its own page.'; host.append(status); }
        });
        host.append(status);
        active = { section, demo, host, select };
      }
      notify();
    }
    for (const button of buttons) {
      button.addEventListener('click', () => select(button.dataset.chartTab));
      button.addEventListener('keydown', event => {
        let index = buttons.indexOf(button);
        if (event.key === 'ArrowRight') index = (index + 1) % buttons.length;
        else if (event.key === 'ArrowLeft') index = (index + buttons.length - 1) % buttons.length;
        else if (event.key === 'Home') index = 0;
        else if (event.key === 'End') index = buttons.length - 1;
        else return;
        event.preventDefault(); select(buttons[index].dataset.chartTab, true);
      });
    }
    toggle.addEventListener('click', () => { playing = !playing; animate(); });
    reduced.addEventListener('change', event => { playing = !event.matches; animate(); });
    document.addEventListener('visibilitychange', animate);
    section.addEventListener('catalog-visibility', animate);
    animate();
  }
  const observer = new IntersectionObserver(entries => {
    for (const entry of entries) {
      if (entry.isIntersecting) visible.add(entry.target); else visible.delete(entry.target);
      entry.target.dispatchEvent(new Event('catalog-visibility'));
    }
    notify();
  });
  for (const section of sections) observer.observe(section);
  document.addEventListener('visibilitychange', notify);
  window.addEventListener('message', event => {
    if (!active || event.origin !== location.origin || event.source !== active.frame.contentWindow || event.data?.kind !== 'catalog-height') return;
    const height = Number(event.data.height);
    if (Number.isFinite(height) && height > 0) active.frame.style.height = `${Math.min(1000, Math.ceil(height))}px`;
  });
}
