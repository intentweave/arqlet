// Every element the page script binds by ID must exist in the page. The app-level regression uses a DOM that creates
// any ID on request, so a removed element would otherwise only fail in a real browser, where the script stops at
// the first missing one and leaves every later control unbound.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';

const read = name => readFileSync(new URL(`./${name}`, import.meta.url), 'utf8');

test('every element ID used by app.js exists in index.html', () => {
  const html = read('index.html');
  const declared = new Set([...html.matchAll(/\bid="([^"]+)"/g)].map(m => m[1]));
  const script = read('app.js');
  const used = new Set([...script.matchAll(/\$\('([A-Za-z0-9_-]+)'\)/g)].map(m => m[1]));
  // IDs built from a template, such as `cand-${n}` or `obj-${k}`, expand from the lists the script iterates.
  for (const n of [1, 2, 3, 4]) used.add(`cand-${n}`);
  const objectives = script.match(/const OBJECTIVES=\[([^\]]+)\]/);
  assert.ok(objectives, 'app.js lists its objectives');
  for (const key of objectives[1].match(/'([^']+)'/g).map(s => s.slice(1, -1))) {
    for (const prefix of ['obj', 'rank', 'up', 'down']) used.add(`${prefix}-${key}`);
  }
  const missing = [...used].filter(id => !declared.has(id)).sort();
  assert.deepEqual(missing, [], `index.html lacks: ${missing.join(', ')}`);
});
