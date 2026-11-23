import {test} from 'node:test';
import assert from 'node:assert/strict';
import {mkdtempSync, mkdirSync, writeFileSync, readFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {releaseCheck, NPM_PACKAGES} from './release-check.mjs';

// A minimal repository with the files the check reads.
function repo({cargo = '0.9.0', dep = '0.9.0', npm = '0.9.0', changelog = '## 0.9.0 — 2026-12-01\n\nNotes.\n\n## 0.8.0 — 2026-11-22\n\nOld.\n'} = {}) {
  const root = mkdtempSync(join(tmpdir(), 'release-check-'));
  writeFileSync(join(root, 'Cargo.toml'), `[workspace]\nmembers = []\n\n[workspace.package]\nversion = "${cargo}"\nedition = "2021"\n\n[workspace.dependencies]\nserde = { version = "1" }\narqlet-model = { path = "crates/model", version = "=${dep}" }\narqlet-layout = { path = "crates/layout", version = "=0.9.0" }\n`);
  for (const p of NPM_PACKAGES) {
    mkdirSync(join(root, 'packages', p), {recursive: true});
    writeFileSync(join(root, 'packages', p, 'package.json'), JSON.stringify({name: p, version: npm}));
  }
  writeFileSync(join(root, 'CHANGELOG.md'), `# Changelog\n\n## Unreleased\n\n${changelog}`);
  return root;
}

test('a consistent release passes and its CHANGELOG section is the notes', () => {
  const {problems, notes} = releaseCheck(repo(), '0.9.0');
  assert.deepEqual(problems, []);
  assert.equal(notes, '## 0.9.0 — 2026-12-01\n\nNotes.\n');
});

test('every mismatch is listed', () => {
  const {problems} = releaseCheck(repo({cargo: '0.8.0', dep: '0.8.0', npm: '0.8.1', changelog: ''}), '0.9.0');
  assert.equal(problems.length, 1 + 1 + NPM_PACKAGES.length + 1, problems.join('\n'));
  assert.ok(problems.some(p => p.includes('arqlet-model: =0.8.0')));
  assert.ok(problems.some(p => p.startsWith('CHANGELOG.md')));
});

test('the repository itself is consistent for its current version', () => {
  const root = new URL('..', import.meta.url).pathname;
  const version = readFileSync(join(root, 'Cargo.toml'), 'utf8').match(/\[workspace\.package\][^[]*?\nversion\s*=\s*"([^"]+)"/)[1];
  assert.deepEqual(releaseCheck(root, version).problems, []);
});
