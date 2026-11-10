# Contributing to Arqlet

Thank you for your interest. Arqlet is a prototype: APIs, the JSON protocol and the
snapshot format are provisional. Please open an issue to discuss larger changes
before you start.

## Before you open a pull request

Run the checks that CI runs:

```sh
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
```

For changes to the browser, WASM, runtime protocol or demo, also run
`sh apps/playground/build.sh examples/fixtures/small.json`.

Project rules that reviews check (details in [CLAUDE.md](CLAUDE.md) and
[AGENTS.md](AGENTS.md)):

- The core crates stay synchronous, deterministic and free of browser, Node,
  file system and network dependencies.
- Logical edge direction is preserved; cycles, self-loops and parallel edges must
  work.
- Stale layout results are never applied silently.
- Performance claims need a reproducible measurement in `benchmarks/results/`.
- Output must be identical for any thread count. Default output stays
  byte-identical unless a change is accepted behind an option first.
- No code copied from other projects without recording its source revision,
  license and notices. Code from EPL- or GPL-licensed layout engines (such as ELK)
  must not be copied at all.

## License of contributions

Arqlet is licensed under either of the Apache License, Version 2.0
([LICENSE-APACHE](LICENSE-APACHE)) or the MIT license ([LICENSE-MIT](LICENSE-MIT)),
at your option. Unless you explicitly state otherwise, any contribution
intentionally submitted for inclusion in the work by you, as defined in the
Apache-2.0 license, shall be dual licensed as above, without any additional terms
or conditions.

## Developer Certificate of Origin (DCO)

Every commit must be signed off. A sign-off certifies the Developer Certificate of
Origin below: you wrote the change, or otherwise have the right to submit it
under the project's license. Add it with `git commit -s`, which appends

```text
Signed-off-by: Your Name <your.email@example.com>
```

using your `user.name` and `user.email`. A real name or a known identity is
expected; anonymous contributions cannot be accepted. To sign off commits you have
already made, use `git commit --amend -s` for the last one or
`git rebase --signoff main` for a branch. A CI check (`.github/workflows/dco.yml`)
rejects pull requests with unsigned commits.

```text
Developer Certificate of Origin
Version 1.1

Copyright (C) 2004, 2006 The Linux Foundation and its contributors.

Everyone is permitted to copy and distribute verbatim copies of this
license document, but changing it is not allowed.


Developer's Certificate of Origin 1.1

By making a contribution to this project, I certify that:

(a) The contribution was created in whole or in part by me and I
    have the right to submit it under the open source license
    indicated in the file; or

(b) The contribution is based upon previous work that, to the best
    of my knowledge, is covered under an appropriate open source
    license and I have the right under that license to submit that
    work with modifications, whether created in whole or in part
    by me, under the same open source license (unless I am
    permitted to submit under a different license), as indicated
    in the file; or

(c) The contribution was provided directly to me by some other
    person who certified (a), (b) or (c) and I have not modified
    it.

(d) I understand and agree that this project and the contribution
    are public and that a record of the contribution (including all
    personal information I submit with it, including my sign-off) is
    maintained indefinitely and may be redistributed consistent with
    this project or the open source license(s) involved.
```
