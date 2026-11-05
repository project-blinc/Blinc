const nudge = document.querySelector('#content-nudge');
const content = document.querySelector('#component-content');
const params = new URLSearchParams(location.search);

if (nudge && content && !params.has('embed') && !params.has('standalone')) {
  const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)');
  const update = () => {
    nudge.hidden = content.getBoundingClientRect().top <= innerHeight - 48;
  };
  let scheduled = false;
  const schedule = () => {
    if (scheduled) return;
    scheduled = true;
    requestAnimationFrame(() => { scheduled = false; update(); });
  };
  nudge.addEventListener('click', event => {
    event.preventDefault();
    content.querySelector('h2').focus({ preventScroll: true });
    content.scrollIntoView({ behavior: reducedMotion.matches ? 'instant' : 'smooth', block: 'start' });
  });
  addEventListener('scroll', schedule, { passive: true });
  addEventListener('resize', schedule);
  new ResizeObserver(schedule).observe(document.querySelector('.main'));
  update();
}
