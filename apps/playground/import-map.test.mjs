// Browser module resolution without a browser: every bare @arqlet/* specifier in a served page or module
// must be covered by that page's import map and point at an existing file; every relative import must resolve
// to a file with an explicit .js extension, because the browser does not add extensions.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync, readdirSync, statSync, existsSync} from 'node:fs';
import {dirname, join, relative, resolve} from 'node:path';
import {fileURLToPath} from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '../..');
const packagesDir = join(repo, 'packages');

// Served URL prefix -> directory on disk, as in server.py (/packages/ maps to packages/).
const served = {'/packages/': packagesDir};

function files(dir, pattern) {
  return readdirSync(dir).flatMap(name => {
    const path = join(dir, name);
    if (name === 'node_modules' || name === 'dist' || name === 'generated') return [];
    if (statSync(path).isDirectory()) return files(path, pattern);
    return pattern.test(name) ? [path] : [];
  });
}

function importMapOf(html) {
  const match = html.match(/<script type="importmap">(.*?)<\/script>/s);
  return match ? JSON.parse(match[1]).imports : {};
}

function specifiers(source) {
  return [...source.matchAll(/(?:^|\s|;|\))(?:import|export)\b[^'";]*?\bfrom\s*'([^']+)'|import\s*'([^']+)'/gm)].map(m => m[1] ?? m[2]);
}

// Resolves a bare specifier through an import map to the served path, then to disk.
function diskPathOf(specifier, imports) {
  for (const [prefix, target] of Object.entries(imports)) {
    if (specifier.startsWith(prefix)) {
      const servedPath = target + specifier.slice(prefix.length);
      const servedPrefix = Object.keys(served).find(p => servedPath.startsWith(p));
      assert.ok(servedPrefix, `${specifier} maps to ${servedPath}, which the server does not expose`);
      return join(served[servedPrefix], servedPath.slice(servedPrefix.length));
    }
  }
  return null;
}

const pages = ['index.html', 'compare.html'].map(name => join(here, name));
const modules = [
  ...files(here, /\.m?js$/),
  ...files(packagesDir, /\.m?js$/),
  ...files(join(repo, 'examples', 'react-flow'), /\.(jsx|js)$/),
];

for (const page of pages) {
  test(`${relative(repo, page)} declares an import map for every workspace package`, () => {
    const imports = importMapOf(readFileSync(page, 'utf8'));
    for (const name of ['viewer', 'engine-client']) {
      assert.ok(imports[`@arqlet/${name}/`], `missing @arqlet/${name}/`);
    }
  });
}

test('every @arqlet import resolves through the page import maps to an existing file', () => {
  const maps = pages.map(page => importMapOf(readFileSync(page, 'utf8')));
  let checked = 0;
  for (const file of modules) {
    if (file.includes(`${join('examples', 'react-flow')}`)) continue; // Vite resolves its own imports
    for (const specifier of specifiers(readFileSync(file, 'utf8'))) {
      if (!specifier.startsWith('@arqlet/')) continue;
      const hits = maps.map(imports => diskPathOf(specifier, imports)).filter(Boolean);
      assert.ok(hits.length > 0, `${relative(repo, file)} imports ${specifier}, which no page import map covers`);
      for (const path of hits) assert.ok(existsSync(path), `${relative(repo, file)} imports ${specifier} -> missing ${path}`);
      checked++;
    }
  }
  assert.ok(checked > 0, 'expected at least one @arqlet import to check');
});

test('relative imports point at existing files with explicit .js extensions', () => {
  for (const file of modules) {
    for (const specifier of specifiers(readFileSync(file, 'utf8'))) {
      if (!specifier.startsWith('.')) continue;
      if (file.includes(`${join('examples', 'react-flow')}`)) continue;
      assert.match(specifier, /\.m?js$/, `${relative(repo, file)} imports ${specifier} without an extension`);
      const target = resolve(dirname(file), specifier);
      assert.ok(existsSync(target), `${relative(repo, file)} imports ${specifier}, which does not exist`);
    }
  }
});

test('worker and WASM URLs are resolved relative to their module or page, not the repository', () => {
  const engine = readFileSync(join(packagesDir, 'engine-client/engine.js'), 'utf8');
  assert.match(engine, /new URL\('\.\/worker\.js', import\.meta\.url\)/);
  assert.ok(existsSync(join(packagesDir, 'engine-client/worker.js')));
});
