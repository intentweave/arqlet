# Releasing to crates.io and npm

Provisional. Arqlet is a prototype; releases are `0.x` and may break any API.

## What is published

| Registry | Package | Contents |
|---|---|---|
| crates.io | `arqlet` | facade: re-exports the engine crates, `dispatch` and `dispatch_streaming` |
| crates.io | `arqlet-model`, `arqlet-topology`, `arqlet-routing`, `arqlet-scene`, `arqlet-view`, `arqlet-layout`, `arqlet-reference`, `arqlet-elk`, `arqlet-runtime` | the engine crates, exact internal versions (`=0.1.0`) |
| npm | `arqlet` | entry point: re-exports engine client and viewer, types from contracts |
| npm | `@arqlet/engine-client` | `arqlet.wasm` (built by `prepack`), loader, worker, ELK-style call, presets |
| npm | `@arqlet/viewer` | SVG renderer, navigation, route helpers, React Flow mapping |
| npm | `@arqlet/contracts` | bundled TypeScript types (`dist/index.d.ts`, built by `prepack`) |

Not published: `arqlet-wasm` (its output ships in `@arqlet/engine-client`), `arqlet-contracts` (tooling), `arqlet-node`
(placeholder), `@arqlet/containment-tree`, `@arqlet/playground`, the examples and the Intentweave exporter.

## Version bump

1. Set `version` in `[workspace.package]` and every `=x.y.z` in `[workspace.dependencies]` of `Cargo.toml`.
2. Set `version` in `packages/{arqlet,engine-client,viewer,contracts}/package.json`. Internal npm dependencies use
   `workspace:^`; pnpm replaces them with the released version.
3. Run the checks in `CLAUDE.md` and `sh apps/playground/build.sh examples/fixtures/small.json`.

## Dry run

```sh
cargo publish --dry-run --workspace --exclude arqlet-wasm --exclude arqlet-contracts --exclude arqlet-node
for p in contracts engine-client viewer arqlet; do (cd packages/$p && pnpm pack --pack-destination /tmp/arqlet-pack); done
```

`prepack` (`tools/npm-prepack.mjs`) copies the license files, builds the WASM for `engine-client` (Rust with
`wasm32-unknown-unknown`) and bundles the contract types. Check a tarball by installing it into an empty project.

## Publish

Publishing cannot be undone (crates.io only allows yanking; npm unpublish is limited to 72 hours).

```sh
cargo login                       # token from crates.io
cargo publish --workspace --exclude arqlet-wasm --exclude arqlet-contracts --exclude arqlet-node
npm login                         # the account that owns the @arqlet organisation
pnpm -r --filter @arqlet/contracts --filter @arqlet/engine-client --filter @arqlet/viewer --filter arqlet publish --access public
git tag v0.1.0 && git push origin v0.1.0
```

`cargo publish --workspace` publishes in dependency order. `pnpm -r publish` publishes in dependency order as well and
skips versions that already exist.

## Later

- A release workflow on tags with trusted publishing (OIDC) for crates.io and npm and npm provenance, so no tokens
  are stored.
- `CHANGELOG.md`.
- A smaller WASM (size-optimised profile or `wasm-opt`); only with unchanged output.
