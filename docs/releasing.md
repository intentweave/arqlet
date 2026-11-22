# Releasing to crates.io and npm

Provisional. Arqlet is a prototype; releases are `0.x` and may break any API.

## What is published

| Registry | Package | Contents |
|---|---|---|
| crates.io | `arqlet` | facade: re-exports the engine crates, `dispatch` and `dispatch_streaming` |
| crates.io | `arqlet-model`, `arqlet-topology`, `arqlet-routing`, `arqlet-scene`, `arqlet-view`, `arqlet-layout`, `arqlet-reference`, `arqlet-elk`, `arqlet-runtime` | the engine crates, exact internal versions (`=x.y.z`) |
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
3. In `CHANGELOG.md`, move the entries under *Unreleased* to a section `## x.y.z — <date>` with a short summary and,
   when anything breaks, an *Upgrading from* section; keep an empty *Unreleased* above it. Update the version in
   `README.md` (status line and the `arqlet = "x.y"` dependency) and the packaging row of `docs/specification.md`.
4. Run the checks in `CLAUDE.md` and `sh apps/playground/build.sh examples/fixtures/small.json`.

## Minimum Rust version

The workspace declares `rust-version` once (`Cargo.toml`), and every crate inherits it. It follows what the dependencies
need and stays well behind stable, so that toolchains pinned by companies, distributions and CI images keep working;
raise it when a dependency or a language feature needs it, not to follow stable. The CI job `msrv` checks the workspace
and the WASM adapter with exactly that version. A change of edition is a separate decision.

## Dry run

```sh
cargo publish --dry-run --workspace --exclude arqlet-wasm --exclude arqlet-contracts --exclude arqlet-node
for p in contracts engine-client viewer arqlet; do (cd packages/$p && pnpm pack --pack-destination /tmp/arqlet-pack); done
```

`prepack` (`tools/npm-prepack.mjs`) copies the license files, builds the WASM for `engine-client` (Rust with
`wasm32-unknown-unknown`) and bundles the contract types. For `engine-client` it also writes `THIRD_PARTY_NOTICES.md`,
the copyright and license notices of the Rust crates linked into the WASM (`tools/third-party-notices.mjs`; a crate
without a license file fails `node tools/third-party-notices.mjs --check`, and a release waits for a decision on it).
Check a tarball by installing it into an empty project.

## Publish

Publishing cannot be undone (crates.io only allows yanking; npm unpublish is limited to 72 hours).

```sh
cargo login                       # token from crates.io
cargo publish --workspace --exclude arqlet-wasm --exclude arqlet-contracts --exclude arqlet-node
npm login                         # the account that owns the @arqlet organisation
pnpm -r --filter @arqlet/contracts --filter @arqlet/engine-client --filter @arqlet/viewer --filter arqlet publish --access public
git tag -a vx.y.z -m x.y.z && git push origin vx.y.z
```

`cargo publish --workspace` publishes in dependency order. `pnpm -r publish` publishes in dependency order as well and
skips versions that already exist.

## The `query` feature

`arqlet-runtime` has an optional feature `query` (the operation `query`, ADR 0021 M5b) that depends on the query crates,
which are `publish = false` for now. `cargo publish` needs every dependency, optional ones too, to exist on the registry,
so `arqlet-runtime` and `arqlet-wasm` cannot be published with the feature declared until `arqlet-query`,
`arqlet-query-cypher` and `arqlet-query-model` are published first (they need the publish metadata, a README and the
licenses of the other crates, and `decypher`'s notice in the npm package). Until then, release from a revision that does
not declare the feature, or publish the query crates in the same release. The npm artifact is built without the feature.

## Later

- A release workflow on tags with trusted publishing (OIDC) for crates.io and npm and npm provenance, so no tokens
  are stored.
- A smaller WASM (size-optimised profile or `wasm-opt`); only with unchanged output.
