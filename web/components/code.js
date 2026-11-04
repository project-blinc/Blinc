import { numbered } from './syntax/format.js';
for (const code of document.querySelectorAll('pre code:not([data-editor-highlight])')) {
  const language = [...code.classList].find(name => name.startsWith('language-'))?.slice(9) || 'text';
  code.parentElement.classList.add('code-body');
  code.innerHTML = numbered(code.textContent, language);
}
for (const button of document.querySelectorAll('[data-copy]')) button.addEventListener('click', async () => {
  const original = button.textContent;
  try {
    await navigator.clipboard.writeText(document.getElementById(button.dataset.copy).textContent);
    button.textContent = 'Copied';
  } catch { button.textContent = 'Select and copy the code below'; }
  setTimeout(() => { button.textContent = original; }, 2000);
});
