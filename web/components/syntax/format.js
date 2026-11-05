import hljs from './highlight.js';

const rust = hljs.getLanguage('rust');
hljs.registerLanguage('blinc-rust', () => ({
  ...rust,
  contains: [
    ...rust.contains.filter(rule => !['[a-zA-Z]\\w*::', '->'].includes(rule.begin)),
    { className: 'macro', begin: /\b[a-zA-Z_]\w*!/, relevance: 0 },
    { className: 'type', begin: /\b[A-Z][a-zA-Z_0-9]*\b/, relevance: 0 },
    { className: 'call', begin: /\b(?!if\b|while\b|match\b|for\b)[a-z_]\w*(?=\s*\()/, relevance: 0 },
    { className: 'punctuation', begin: /::|->|[{}[\](),;.:<>|&=*+\/-]/, relevance: 0 },
  ],
}));

export function highlight(source, language) {
  const name = language === 'rust' ? 'blinc-rust' : language;
  if (!hljs.getLanguage(name)) return source.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');
  return hljs.highlight(name, source, true).value;
}

export function numbered(source, language, separator = '\n') {
  const open = [], lines = [];
  let line = '';
  for (const token of highlight(source, language).match(/<span[^>]*>|<\/span>|\n|[^<\n]+/g) || []) {
    if (token === '\n') {
      lines.push(line + '</span>'.repeat(open.length));
      line = open.join('');
    } else {
      if (token.startsWith('<span')) open.push(token);
      else if (token === '</span>') open.pop();
      line += token;
    }
  }
  lines.push(line);
  return lines.map(value => `<span class="code-line">${value}</span>`).join(separator);
}
