// Release check (#120): the version of a release must be the same everywhere before anything is published.
//
//   node tools/release-check.mjs 0.9.0                 checks the version (exit 1 with every mismatch listed)
//   node tools/release-check.mjs v0.9.0 --notes FILE   also writes the CHANGELOG section of that version to FILE
//
// Checked: `version` in `[workspace.package]` of Cargo.toml, every internal `=x.y.z` in `[workspace.dependencies]`, the
// `version` of the published npm packages, and a `## x.y.z — date` section in CHANGELOG.md.
import {readFileSync, writeFileSync} from 'node:fs';
import {join, dirname} from 'node:path';
import {fileURLToPath} from 'node:url';

export const NPM_PACKAGES = ['contracts', 'engine-client', 'viewer', 'arqlet'];

/** The mismatches for `version` in the repository at `root` (empty when it is consistent), and the CHANGELOG section. */
export function releaseCheck(root, version) {
  const problems = [];
  const cargo = readFileSync(join(root, 'Cargo.toml'), 'utf8');
  const pkg = cargo.match(/\[workspace\.package\][^[]*?\nversion\s*=\s*"([^"]+)"/);
  if (!pkg) problems.push('Cargo.toml: no version in [workspace.package]');
  else if (pkg[1] !== version) problems.push(`Cargo.toml [workspace.package]: version ${pkg[1]}, expected ${version}`);
  const deps = cargo.split(/\n\[/).find(s => s.startsWith('workspace.dependencies]')) ?? '';
  for (const m of deps.matchAll(/^(arqlet[\w-]*)\s*=\s*\{[^}]*path\s*=\s*"[^"]+"[^}]*version\s*=\s*"=([^"]+)"/gm)) {
    if (m[2] !== version) problems.push(`Cargo.toml [workspace.dependencies] ${m[1]}: =${m[2]}, expected =${version}`);
  }
  if (!/^arqlet[\w-]*\s*=\s*\{[^}]*path/m.test(deps)) problems.push('Cargo.toml: no internal workspace dependencies found');
  for (const p of NPM_PACKAGES) {
    const json = JSON.parse(readFileSync(join(root, 'packages', p, 'package.json'), 'utf8'));
    if (json.version !== version) problems.push(`packages/${p}/package.json (${json.name}): ${json.version}, expected ${version}`);
  }
  const changelog = readFileSync(join(root, 'CHANGELOG.md'), 'utf8');
  const escaped = version.replace(/[.+]/g, '\\$&');
  const start = changelog.search(new RegExp(`^## ${escaped}( |$)`, 'm'));
  let notes = '';
  if (start < 0) problems.push(`CHANGELOG.md: no section "## ${version} — <date>"`);
  else {
    const rest = changelog.slice(start);
    const next = rest.slice(3).search(/^## /m);
    notes = (next < 0 ? rest : rest.slice(0, next + 3)).trim() + '\n';
  }
  return {problems, notes};
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const arg = process.argv[2];
  if (!arg) { console.error('usage: node tools/release-check.mjs VERSION [--notes FILE]'); process.exit(2); }
  const version = arg.replace(/^v/, '');
  const root = join(dirname(fileURLToPath(import.meta.url)), '..');
  const {problems, notes} = releaseCheck(root, version);
  if (problems.length) {
    console.error(`Release ${version} is not consistent:`);
    for (const p of problems) console.error(`- ${p}`);
    process.exit(1);
  }
  const at = process.argv.indexOf('--notes');
  if (at > 0) writeFileSync(process.argv[at + 1], notes);
  console.log(`Release ${version}: Cargo.toml, the internal crate versions, ${NPM_PACKAGES.length} npm packages and CHANGELOG.md agree.`);
}
