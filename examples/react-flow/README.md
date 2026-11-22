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

Large views: the checkbox *GPU far layer* (on by itself from 1 500 elements) draws the scene with `@arqlet/viewer`'s
scene view: from afar on the GPU (WebGPU, else WebGL 2), up close React Flow with the part in view and a viewport set
from the view (`reactFlowViewport`), so nothing moves at the switch (ADR 0025). `ARQLET_BACKEND` points the development
proxy at another port than 8765.

The *Style* select shows the shared appearance (`@arqlet/viewer/style.js`): `toReactFlow(scene, {style})` puts each
node's and edge's resolved colours in `data.look`, which the node and edge types draw.

This is a reference viewer, not an editor. Node dragging and edge reconnection
are disabled. Both viewers use `@arqlet/viewer/scene.js`; supplied routes are never
recomputed by React Flow. See [scene contract](../../docs/scene-viewers.md).
