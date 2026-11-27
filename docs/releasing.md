# Releasing to crates.io and npm

Provisional. Arqlet is a prototype; releases are `0.x` and may break any API.

## What is published

| Registry | Package | Contents |
|---|---|---|
| crates.io | `arqlet` | facade: re-exports the engine crates, `dispatch` and `dispatch_streaming` |
| crates.io | `arqlet-model`, `arqlet-topology`, `arqlet-routing`, `arqlet-scene`, `arqlet-view`, `arqlet-layout`, `arqlet-reference`, `arqlet-elk`, `arqlet-runtime`, and (provisional, since 0.8.0) `arqlet-query`, `arqlet-query-cypher`, `arqlet-query-model`, `arqlet-query-json` | the engine crates, exact internal versions (`=x.y.z`) |
| npm | `arqlet` | entry point: re-exports engine client and viewer, types from contracts |
| npm | `@arqlet/engine-client` | `arqlet.wasm` (built by `prepack`), loader, worker, ELK-style call, presets |
| npm | `@arqlet/viewer` | SVG renderer, navigation, route helpers, React Flow mapping |
| npm | `@arqlet/contracts` | bundled TypeScript types (`dist/index.d.ts`, built by `prepack`) |
| npm | `@arqlet/query` (provisional) | `arqlet-query.wasm` (built by `prepack`), loader and types: queries without the layout engine |

Not published: `arqlet-wasm` (its output ships in `@arqlet/engine-client`), `arqlet-contracts` (tooling), `arqlet-node`
(placeholder), `arqlet-query-cli` (provisional tool of the query layer), `arqlet-query-wasm` (its output ships in
`@arqlet/query`), `@arqlet/containment-tree`, `@arqlet/playground`, the examples and the Intentweave exporter.

## Version bump

1. Set `version` in `[workspace.package]` and every `=x.y.z` in `[workspace.dependencies]` of `Cargo.toml`.
2. Set `version` in `packages/{arqlet,engine-client,viewer,contracts,query}/package.json`. Internal npm dependencies use
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
cargo publish --dry-run --workspace --exclude arqlet-wasm --exclude arqlet-contracts --exclude arqlet-node \
  --exclude arqlet-query-cli --exclude arqlet-query-wasm
for p in contracts engine-client viewer arqlet query; do (cd packages/$p && pnpm pack --pack-destination /tmp/arqlet-pack); done
```

`prepack` (`tools/npm-prepack.mjs`) copies the license files, builds the WASM for `engine-client` and `query` (Rust with
`wasm32-unknown-unknown`) and bundles the contract types. For those two packages it also writes
`THIRD_PARTY_NOTICES.md`, the copyright and license notices of the Rust crates linked into their WASM
(`tools/third-party-notices.mjs`; a crate without a license file fails `node tools/third-party-notices.mjs --check`, for
the query module with `--package arqlet-query-wasm`, and a release waits for a decision on it).
Check a tarball by installing it into an empty project.

## Publish with the release workflow

Publishing cannot be undone (crates.io only allows yanking; npm unpublish is limited to 72 hours).

After the version bump is merged to `main`, a tag publishes the release (`.github/workflows/release.yml`, #120):

```sh
node tools/release-check.mjs x.y.z     # every version agrees (the workflow checks it again)
git tag -a vx.y.z -m x.y.z && git push origin vx.y.z
```

The workflow then runs, in this order:

1. `verify`: the release check (and the CHANGELOG section as the release notes), `cargo fmt --check`, the workspace
   tests, the dry run of the crates (`cargo publish --dry-run --workspace`), and the npm packages packed with
   `pnpm pack`, installed into an empty project and imported.
2. `crates`: the crates not yet on crates.io, with trusted publishing (`rust-lang/crates-io-auth-action`: a short-lived
   token from GitHub's OIDC token; no token is stored).
3. `npm`: the packages not yet on npm, in dependency order: `pnpm pack` (it runs `prepack` and resolves `workspace:^`
   to the released version), then `npm publish` of the tarball with trusted publishing; npm adds provenance itself.
4. `github-release`: the GitHub release of the tag, with the CHANGELOG section.

The publishing jobs run in the GitHub environment `release`. Each skips versions the registry already has, so a run that
failed half way is run again from the Actions page. `crates` logs in to crates.io only when a crate is left to publish,
so a tag after a release by hand works without trusted publishing. Run by hand (`workflow_dispatch`, with a version),
the workflow only runs `verify`: a dry run that never publishes.

Afterwards, [check the release](#check-the-release).

### One-time setup

Trusted publishing is configured per package on the registries. Each package must exist there already (all of them do
since 0.8.0 except `@arqlet/query`); a new crate or npm package is published once by hand (below) before it can be added.
Until that is done for `@arqlet/query`, the workflow's `npm` job fails at it; it comes last, so the other packages are
published, and the job is run again after the setup. The first release of a package that is already tagged is published
from the checked-out tag, which needs `--no-git-checks` (without it pnpm refuses a detached HEAD):
`pnpm --filter <package> publish --access public --no-git-checks`.

- **GitHub:** an environment `release` (Settings → Environments). Required reviewers on it make every release wait for
  an approval before anything is published.
- **crates.io:** for every published crate (`arqlet`, `arqlet-model`, `arqlet-topology`, `arqlet-routing`,
  `arqlet-scene`, `arqlet-view`, `arqlet-layout`, `arqlet-reference`, `arqlet-elk`, `arqlet-runtime`, `arqlet-query`,
  `arqlet-query-cypher`, `arqlet-query-model`, `arqlet-query-json`): Settings → Trusted Publishing → add GitHub, owner
  `intentweave`, repository `arqlet`, workflow `release.yml`, environment `release`.
- **npm:** for `arqlet`, `@arqlet/contracts`, `@arqlet/engine-client`, `@arqlet/viewer` and `@arqlet/query`: Settings → Trusted
  publisher → GitHub Actions, organization `intentweave`, repository `arqlet`, workflow `release.yml`, environment
  `release`. Then the package can forbid token publishing (Settings → Publishing access).

Requirements the workflow meets: `id-token: write` on the publishing jobs, npm 11.5.1 or later, and no `registry-url`
in `setup-node` (it writes a token into `.npmrc`, which keeps npm from using OIDC).

## Publish by hand (fallback)

For a first release of a new package, or when the workflow cannot be used.

```sh
cargo login                       # token from crates.io
cargo publish --workspace --exclude arqlet-wasm --exclude arqlet-contracts --exclude arqlet-node \
  --exclude arqlet-query-cli --exclude arqlet-query-wasm
npm login                         # the account that owns the @arqlet organisation
pnpm -r --filter @arqlet/contracts --filter @arqlet/engine-client --filter @arqlet/viewer --filter arqlet --filter @arqlet/query publish --access public
git tag -a vx.y.z -m x.y.z && git push origin vx.y.z
```

`cargo publish --workspace` publishes in dependency order. `pnpm -r publish` publishes in dependency order as well and
skips versions that already exist. Push the tag only when the registries show every package (see below): it starts the
release workflow too, which finds every version published already and only creates the GitHub release. A package that
is still missing would be published by the workflow, which needs trusted publishing.

## Check the release

A release is done only when it installs from the registries. A publish command that ran is not enough: a package can be
missing after it, and a new version can take some minutes to appear.

```sh
v=x.y.z
# 1. Every npm package has the version.
for p in arqlet @arqlet/engine-client @arqlet/viewer @arqlet/contracts @arqlet/query; do
  npm view "$p@$v" version >/dev/null 2>&1 && echo "$p $v" || echo "$p MISSING"
done
# 2. The npm entry point installs into an empty project and imports.
(cd "$(mktemp -d)" && npm init -y >/dev/null && npm install --no-audit --no-fund "arqlet@$v" &&
  node --input-type=module -e "if (!Object.keys(await import('arqlet')).length) throw Error('arqlet exports nothing')")
# 3. The facade crate resolves from crates.io and builds with its dependencies.
(cd "$(mktemp -d)" && cargo init --quiet --name release-check && cargo add "arqlet@=$v" && cargo build)
```

The query crates are not dependencies of the facade; look them up separately (`cargo info arqlet-query-cypher@x.y.z`,
likewise for the other three).

## The `query` feature

`arqlet-runtime` has an optional feature `query` (the operation `query`, ADR 0021 M5b) that depends on the query crates.
`cargo publish` needs every dependency, optional ones too, to exist on the registry, so since 0.8.0 `arqlet-query`,
`arqlet-query-cypher`, `arqlet-query-model` and `arqlet-query-json` are published (provisional, with the publish metadata,
a README and the licenses of the other crates); `cargo publish --workspace` publishes them before `arqlet-runtime`. Their
one third-party dependency, `decypher`, is recorded in `docs/architecture.md`. The engine's npm artifact is built without
the feature, so `decypher` is not in the engine package and its notices need no entry there. It is in `@arqlet/query`,
whose notices `prepack` writes.

## Later

- A smaller WASM (size-optimised profile or `wasm-opt`); only with unchanged output.
