# React Flow reference client

Start the existing loopback backend first (repository root):

```sh
sh apps/playground/build.sh
python3 apps/playground/server.py
```

Then, in another terminal:

```sh
pnpm install --frozen-lockfile --ignore-scripts
cd examples/react-flow
pnpm run dev
```

Open http://127.0.0.1:5174. The dev server proxies generated snapshots, shared profiles, the WASM engine and versioned
engine commands to port 8765. Scene geometry is computed by the engine (`scene` operation). You can also open a snapshot exported by the SVG
demo. Select a container and use the open/close button. File loading does not
upload a snapshot; an explicit container action sends it to the local backend.

`pnpm run build` verifies bundling; `pnpm test` runs the shared adapter fixtures.
The production bundle needs a host providing `/generated/snapshot.json` and
`/api/op`; Vite's development proxy is not included in the bundle.

This is a reference viewer, not an editor. Node dragging and edge reconnection
are disabled. Both viewers use `@arqlet/viewer/scene.js`; supplied routes are never
recomputed by React Flow. See [scene contract](../../docs/scene-viewers.md).
