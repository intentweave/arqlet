# @arqlet/node (provisional, unpublished)

The Arqlet graph-layout engine as a native Node.js module (Node-API through napi-rs, `crates/node`), with the interface
of the WASM engine of `@arqlet/engine-client` and a WASM fallback. Decided in
[ADR 0030](../../docs/adr/0030-native-node-adapter.md).

- Every call that computes runs on libuv's thread pool, so the event loop keeps running during a layout.
- Sessions keep their snapshot in the module. Different sessions run in parallel, and each session runs its requests
  one at a time, in the order sent.
- The answers are the same bytes as the WASM engine's (`node tools/platform-parity.mjs --node`).

```js
import {loadEngine, loadNativeEngine} from '@arqlet/node';
import {createSession} from '@arqlet/engine-client/session.js';

const run = await loadEngine({engine: 'auto'});   // 'native', 'wasm' or 'auto'; default ARQLET_ENGINE, else 'auto'
const session = createSession(run);
await session.import(graph, {limit: 2000});
```

`loadNativeEngine({sessionBytes})` caps the total estimated weight of the sessions. Over that cap, the least recently
used idle session is dropped. `run.sessions()` lists the open sessions.

## Build

No prebuilt binaries are published yet. To build the module for this machine, run:

```sh
node packages/node/build.mjs            # release; --profile fast for tests; --features query for the query operation
node --test packages/node/index.test.mjs
```

The loader looks for the module in this order:

1. the `path` given;
2. `ARQLET_NODE_ADDON`;
3. `arqlet.<platform>-<arch>.node` beside `index.js`;
4. the platform package `@arqlet/node-<key>`, once that is published.

## Accepted gaps (ADR 0030, section 8)

- **No prebuilt binaries yet.** Publishing is a separate step (ADR 0030, sections 3 and 9); until then the package is
  `private`.
- **Progressive publications are not delivered.** `refineProgressive` answers with its final result only, as the WASM
  engine does without `onPublish`.
- **Phase timings are per process.** With `timings: true` on calls that run at the same time, their times add up. The
  answers themselves are unaffected.
- **No musl Linux or Intel macOS binaries are planned.** On those systems, build from source with `build.mjs`, or let
  `loadEngine({engine: 'auto'})` fall back to WASM.

License: MIT OR Apache-2.0.
