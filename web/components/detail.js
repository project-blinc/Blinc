import { LiveDemo } from './runtime/host.js';
import './code.js';
import './chart-sections.js';
import './content-nudge.js';
import { numbered } from './syntax/format.js';

const configNode = document.querySelector('#component-config');
if (configNode) {
  const config = JSON.parse(configNode.textContent);
  document.querySelector('.css-editor').hidden = config.customCss === false;
  const primary = document.querySelector('.example');
  primary.style.setProperty('--demo-height', `${config.height}px`);
  const tabs = [...primary.querySelectorAll('[role="tab"]')];
  const panels = [...primary.querySelectorAll('[role="tabpanel"]')];
  const status = document.querySelector('#live-status');
  const fallback = document.querySelector('#live-fallback');
  const retry = document.querySelector('#retry-live');
  const stage = document.querySelector('#live-canvas');
  const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)');
  let tab = 'preview', runtime, playing = config.animated !== false && !reducedMotion.matches, error = false;
  let inView = true, hostVisible = !new URLSearchParams(location.search).has('embed');
  const updateVisibility = () => runtime?.setVisible(tab === 'live' && inView && hostVisible);
  const dark = document.querySelector('#demo-dark');
  const disabled = document.querySelector('#demo-disabled');
  const progress = document.querySelector('#demo-progress');
  const themes = [...document.querySelectorAll('[data-theme-family]')];
  const cssEditor = document.querySelector('#custom-css');
  const cssStatus = document.querySelector('#css-status');
  const cssHighlight = document.querySelector('#css-highlight');
  const applyCss = document.querySelector('#apply-css');
  let appliedCss = '';
  let cssRequest = 0, pendingCss;
  const starterCss = cssEditor.value;
  const renderCss = () => { cssHighlight.innerHTML = numbered(cssEditor.value, 'css', ''); };
  renderCss();
  dark.checked = document.documentElement.dataset.scheme !== 'light';
  document.querySelector('#scheme-label').textContent = dark.checked ? 'Dark' : 'Light';
  let family = 'hybrid';
  const gifButton = document.querySelector('#toggle-animation');
  function updateImages() {
    for (const image of primary.querySelectorAll('[data-animation]')) {
      const fallback = image.closest('#live-fallback');
      const active = image.closest('[role="tabpanel"]').id === `panel-${tab}` && (!fallback || !fallback.hidden);
      image.src = active && playing && inView && !document.hidden ? image.dataset.animation : image.dataset.poster;
    }
    gifButton.textContent = playing ? 'Pause preview' : 'Play preview';
    gifButton.setAttribute('aria-pressed', String(playing));
  }
  function selectTab(next, focus = false) {
    tab = next;
    for (const button of tabs) {
      const selected = button.dataset.tab === tab;
      button.setAttribute('aria-selected', String(selected));
      button.tabIndex = selected ? 0 : -1;
      if (selected && focus) button.focus();
    }
    for (const panel of panels) panel.hidden = panel.id !== `panel-${tab}`;
    updateVisibility();
    updateImages();
  }
  for (const button of tabs) {
    button.addEventListener('click', () => selectTab(button.dataset.tab));
    button.addEventListener('keydown', event => {
      let index = tabs.indexOf(button);
      if (event.key === 'ArrowRight') index = (index + 1) % tabs.length;
      else if (event.key === 'ArrowLeft') index = (index + tabs.length - 1) % tabs.length;
      else if (event.key === 'Home') index = 0;
      else if (event.key === 'End') index = tabs.length - 1;
      else return;
      event.preventDefault(); tabs[index].click(); tabs[index].focus();
    });
  }
  function start() {
    if (config.live === false) {
      status.textContent = 'Live playback is not available here. Explore the native preview and source.';
      stage.dataset.status = 'native';
      stage.setAttribute('aria-busy', 'false');
      primary.querySelector('.controls').hidden = true;
      primary.querySelector('.css-editor').hidden = true;
      return;
    }
    runtime?.stop();
    error = false;
    retry.hidden = true; fallback.hidden = false;
    status.textContent = 'Loading live preview…';
    stage.dataset.status = 'loading';
    stage.setAttribute('aria-busy', 'true');
    runtime = new LiveDemo(stage, {
      ...config, dark: dark.checked, family, css: appliedCss,
      disabled: disabled.checked, progress: Number(progress.value),
    }, (type, data) => {
      if (type === 'progress') status.textContent = 'Loading live preview…';
      if (type === 'ready') {
        runtime.send('theme', { dark: dark.checked, family });
        runtime.send('props', { disabled: disabled.checked, progress: Number(progress.value) });
        status.textContent = 'Press Esc to return to the page controls.';
        stage.dataset.backend = data.backend;
        stage.dataset.status = 'ready';
        fallback.hidden = true;
        updateImages();
        stage.setAttribute('aria-busy', 'false');
        if (appliedCss) cssStatus.textContent = 'Applied';
        if (pendingCss) runtime.send('css', pendingCss);
      }
      if (type === 'css-result' && data.request === pendingCss?.request) {
        if (data.ok) {
          appliedCss = pendingCss.css;
          cssStatus.textContent = appliedCss.trim() ? 'Applied' : 'Default styles';
          delete cssStatus.dataset.error;
        } else {
          cssStatus.textContent = data.message;
          cssStatus.dataset.error = 'true';
        }
        pendingCss = undefined;
        applyCss.disabled = cssEditor.value === appliedCss;
      }
      if (type === 'media-error') { status.textContent = payload.message; return; }
      if (type === 'error') {
        error = true;
        status.textContent = 'Live preview is unavailable. Try again.';
        stage.dataset.status = 'error';
        console.warn('Blinc preview:', data.message);
        if (data.message.startsWith('CSS:')) {
          cssStatus.textContent = data.message;
          cssStatus.dataset.error = 'true';
        }
        stage.setAttribute('aria-busy', 'false');
        fallback.hidden = false; retry.hidden = false;
        updateImages();
      }
    });
    updateVisibility();
  }
  stage.addEventListener('demo-exit', () => document.querySelector('#reset-demo').focus());
  retry.addEventListener('click', start);
  document.querySelector('#reset-demo').addEventListener('click', () => {
    disabled.checked = false; progress.value = 42;
    start();
  });
  for (const button of themes) button.addEventListener('click', () => {
    if (button.dataset.themeFamily === family) return;
    family = button.dataset.themeFamily;
    for (const item of themes) item.setAttribute('aria-pressed', String(item === button));
    if (runtime?.ready) runtime.send('theme', { dark: dark.checked, family });
  });
  function applyStyles() {
    pendingCss = { css: cssEditor.value, request: ++cssRequest };
    cssStatus.textContent = 'Applying…';
    delete cssStatus.dataset.error;
    applyCss.disabled = true;
    if (runtime?.ready) runtime.send('css', pendingCss);
  }
  cssEditor.addEventListener('input', () => {
    renderCss();
    applyCss.disabled = cssEditor.value === appliedCss;
    cssStatus.textContent = applyCss.disabled ? 'Applied' : 'Not applied';
    delete cssStatus.dataset.error;
  });
  cssEditor.addEventListener('scroll', () => {
    cssHighlight.parentElement.scrollTop = cssEditor.scrollTop;
    cssHighlight.parentElement.scrollLeft = cssEditor.scrollLeft;
  });
  cssEditor.addEventListener('keydown', event => {
    if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') {
      event.preventDefault(); applyStyles();
    }
  });
  applyCss.addEventListener('click', applyStyles);
  document.querySelector('#clear-css').addEventListener('click', () => {
    cssEditor.value = ''; applyStyles();
    cssEditor.value = starterCss; renderCss(); applyCss.disabled = !starterCss;
  });
  dark.addEventListener('change', () => {
    document.querySelector('#scheme-label').textContent = dark.checked ? 'Dark' : 'Light';
    if (runtime?.ready) runtime.send('theme', { dark: dark.checked, family });
  });
  document.addEventListener('page-scheme-change', event => {
    dark.checked = event.detail === 'dark';
    dark.dispatchEvent(new Event('change'));
  });
  for (const control of [disabled, progress]) control.addEventListener('input', () => {
    if (runtime?.ready) runtime.send('props', { disabled: disabled.checked, progress: Number(progress.value) });
  });
  gifButton.addEventListener('click', () => { playing = !playing; updateImages(); });
  reducedMotion.addEventListener('change', e => { playing = config.animated !== false && !e.matches; updateImages(); });
  document.addEventListener('visibilitychange', () => { updateVisibility(); updateImages(); });
  window.addEventListener('pagehide', () => runtime?.stop());
  window.addEventListener('pageshow', event => { if (event.persisted) start(); });
  const params = new URLSearchParams(location.search);
  if (params.has('standalone')) document.body.classList.add('standalone');
  if (typeof IntersectionObserver !== 'undefined') {
    new IntersectionObserver(entries => { inView = entries[0].isIntersecting; updateVisibility(); updateImages(); }).observe(primary);
  }
  if (params.has('embed')) {
    document.body.classList.add('embedded');
    window.addEventListener('message', event => {
      if (event.origin !== location.origin || event.source !== parent || event.data?.kind !== 'catalog-visibility') return;
      hostVisible = event.data.visible === true; updateVisibility();
    });
    const reportHeight = () => parent.postMessage({ kind: 'catalog-height', height: Math.ceil(primary.getBoundingClientRect().height) }, location.origin);
    new ResizeObserver(reportHeight).observe(primary);
    reportHeight();
  }
  selectTab(params.get('tab') === 'live' ? 'live' : 'preview');
  // The runtime warms one frame on page visit, independently of the selected tab.
  start();
}
