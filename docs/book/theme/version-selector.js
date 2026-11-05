(async () => {
  const script = [...document.scripts].find(node => /\/version-selector(?:-[a-f0-9]+)?\.js(?:\?|$)/.test(node.src));
  if (!script) return;
  const base = new URL(typeof path_to_root === 'string' ? path_to_root : '.', location.href);
  try {
    const [currentResponse, versionsResponse] = await Promise.all([
      fetch(new URL('book-version.json', base)),
      fetch('/docs/versions.json'),
    ]);
    if (!currentResponse.ok || !versionsResponse.ok) return;
    const current = await currentResponse.json();
    const versions = await versionsResponse.json();
    const entries = versions.editions.filter(item =>
      /^(?:dev|\d+\.\d+\.\d+)$/.test(item.id) && item.url === `/docs/${item.id}/` && Array.isArray(item.pages));
    if (!entries.some(item => item.id === current.id)) return;
    const host = document.querySelector('#menu-bar .right-buttons');
    if (!host || document.querySelector('#book-version')) return;
    const wrapper = document.createElement('label');
    wrapper.className = 'book-version-picker';
    const title = document.createElement('span');
    title.textContent = 'Version';
    const select = document.createElement('select');
    select.id = 'book-version';
    select.setAttribute('aria-label', 'Documentation version');
    for (const entry of entries) {
      const option = document.createElement('option');
      option.value = entry.id;
      option.textContent = entry.id === 'dev' ? 'Development' : entry.id;
      option.selected = entry.id === current.id;
      select.append(option);
    }
    select.addEventListener('change', () => {
      const entry = entries.find(item => item.id === select.value);
      if (!entry) return;
      const chapter = location.pathname.slice(base.pathname.length) || 'index.html';
      const exists = entry.pages.includes(chapter);
      location.assign(entry.url + (exists ? chapter : 'index.html') + (exists ? location.search + location.hash : ''));
    });
    wrapper.append(title, select);
    host.prepend(wrapper);
  } catch {
    // A standalone book build also works without the edition manifest.
  }
})();
