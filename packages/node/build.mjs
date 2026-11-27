#!/usr/bin/env node
// Builds the native module of this platform from crates/node and copies it beside the loader as
// `arqlet.<platform>-<arch>.node` (git-ignored), where `loadNativeEngine()` finds it.
//
//   node packages/node/build.mjs [--profile release|fast] [--features query]
//
// `release` (the default) is the build the parity check and the measurements use, the same profile as the native
// binaries; `fast` builds quicker for tests (the same arithmetic, so the same answers).
import {execFileSync} from 'node:child_process';
import {copyFileSync} from 'node:fs';
import {join, resolve} from 'node:path';

const root = resolve(import.meta.dirname, '../..');
const args = process.argv.slice(2);
const option = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : fallback;
};
const profile = option('--profile', 'release');
const features = option('--features', '');
const cargoArgs = ['build', '-p', 'arqlet-node', '--profile', profile, '--locked', ...(features ? ['--features', features] : [])];
execFileSync('cargo', cargoArgs, {cwd: root, stdio: 'inherit'});
const library = {darwin: 'libarqlet_node.dylib', win32: 'arqlet_node.dll'}[process.platform] ?? 'libarqlet_node.so';
const built = join(root, 'target', profile === 'dev' ? 'debug' : profile, library);
const target = join(import.meta.dirname, `arqlet.${process.platform}-${process.arch}.node`);
copyFileSync(built, target);
console.log(`${built} -> ${target}`);
