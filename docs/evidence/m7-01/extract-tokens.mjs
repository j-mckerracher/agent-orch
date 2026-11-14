// Enumerate declarations, including overridden and inline values; do not infer the cascade.
import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

const desktop = new URL('../../../apps/desktop/', import.meta.url);
const require = createRequire(new URL('package.json', desktop));
const postcss = require('postcss');
const wireframe = process.argv[2] ?? '/Users/mckerracher.joshua/.copilot/workspaces/0eca0ef2-67ee-4694-ba92-183173726df4/attachments/9a4ec9b6-3bf2-460c-85ba-c81a1b79751c-wireframes.html';
const html = readFileSync(wireframe, 'utf8');
const sha256 = createHash('sha256').update(html).digest('hex');
if (sha256 !== '24e819fd145304d73582a78bc6a6848641d560ccd2ccb7ddf2cbd68139735cbf') throw new Error(`Unexpected wireframe SHA-256: ${sha256}`);
function extract(css, source) {
  const found = [];
  postcss.parse(css, { from: source }).walkDecls(/^--/, (decl) => {
    const contexts = [];
    for (let node = decl.parent; node && node.type !== 'root'; node = node.parent) {
      contexts.unshift(node.type === 'rule' ? node.selector : `@${node.name} ${node.params}`);
    }
    found.push({ name: decl.prop, value: decl.value, context: contexts.join(' > '), line: decl.source.start.line });
  });
  return found;
}
const styles = [...html.matchAll(/<style\b[^>]*>([\s\S]*?)<\/style\s*>/gi)].flatMap((m, i) => extract(m[1], `style-${i + 1}`));
const inline = [...html.matchAll(/\bstyle\s*=\s*(["'])(.*?)\1/gi)].flatMap((m, i) => extract(`x{${m[2]}}`, `inline-${i + 1}`).map(({ name, value }) => ({ name, value, context: 'inline style' })));
const tokenPath = fileURLToPath(new URL('src/ui/theme/tokens.css', desktop));
const tokens = extract(readFileSync(tokenPath, 'utf8'), tokenPath);
console.log(JSON.stringify({ sha256, wireframe: { styles, inline }, tokens }, null, 2));
