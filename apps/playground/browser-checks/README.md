# Browser checks of the curation editor

Scripts that drive the playground in a real Chrome with real mouse and keyboard input. They use the DevTools protocol
directly, with no npm dependencies, and they are **not** part of CI: run them by hand after a change to the editor's
gestures.

- `smoke.mjs`: draft creation, click and Ctrl/Cmd-click selection, Escape, a note in the inspector, the pin button.
- `drags.mjs`: lock and unlock of a route, node drag, corner resize, and the refusal of a node move under a locked route.
- `gestures.mjs`: size pin, Alt-click dogleg, segment, container, port, label and joint-selection drags.
- `canvas-controls.mjs`: the pin control of a leaf and of a container, and the collapse and expand controls of a container, with real clicks; a press on a control must not select, drag or pan.
- `containment-tree.mjs`: tree rows, row and canvas selection, arrow keys, container size and strategy from the form, filter.
- `drag-capture.mjs`: a leaf drag over 20 moves; every move must reach the drawing and the node must follow the whole path. The viewport's pan must not take the press.

- `gesture-robustness.mjs`: gestures that end badly: a release outside the window, `pointercancel`, Escape, an engine error and a model change in the middle of a drag. After each, nothing may be left running and the next drag must work. It starts its own server and Chrome (`launch.mjs`). Against the previous `app.js`, the `pointercancel` and Escape checks fail (the node stayed 70 and 60 px away); the release outside the window passed there too, because the pointer capture caught it.
- `latency.mjs`: pointer-to-preview and release-to-result times of a drag, and the number of messages to the workers. Prints one JSON line; see the results in `benchmarks/results/`.
- `launch.mjs`: starts the server and a headless Chrome for the scripts above (`node launch.mjs` keeps them running).

A check whose precondition is missing in the draft is reported as `SKIPPED`, never as a pass.

## Run

1. Build the playground once: `sh apps/playground/build.sh examples/fixtures/small.json`.
2. Start the server on a free port: `python3 apps/playground/server.py --port 8766`.
3. Start Chrome with the DevTools port open, for example on macOS:
   `"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --headless=new --remote-debugging-port=9333 --user-data-dir=/tmp/lw-chrome about:blank`
4. Run a script: `node apps/playground/browser-checks/gestures.mjs 9333 http://127.0.0.1:8766/`

Each script prints one line per check and exits non-zero when a check fails.

## Notes

- On macOS, Ctrl with the primary button is a right-click, so the multi-select uses Cmd (Meta).
- Clicks are matched by the browser's own hit testing. A route line that a node covers is not clickable; the scripts
  look for a visible point on the line first.
- The scripts assume the default example graph and its layout. They do not check pixels.

## Direct view (milestones M1 to M7)

- `gesture-robustness.mjs`, `history.mjs`, `selection-move.mjs`, `port-drag.mjs`, `label-drag.mjs` each start their own server and Chrome (`launch.mjs`, shared client `cdp.mjs`) and exit non-zero on a failed check. Run them one after the other after a change to gestures, the viewer or the history; they found a real regression once (port drag targets that covered the nodes of a large view).
- `port-drag.mjs` and `label-drag.mjs` use the small example `state-order`: in a view of many nodes a port or a label is a target of a few pixels.
- The page loads `generated/arqlet.wasm`: after a change to the engine run `sh apps/playground/build.sh` first, or the page runs the old engine.
