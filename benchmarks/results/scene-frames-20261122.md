# Frame times of the far layer (ADR 0025, V1), 2026-11-22

Panning and zooming a large scene with the WebGL 2 and WebGPU far layer against SVG, in headless Chrome on one machine.
Raw lines: `scene-frames-20261122T134031Z.jsonl`. Script: `apps/playground/browser-checks/scene-frames.mjs`.

- Apple M3, 16 GiB, macOS (Darwin 25.0.0); Chrome 157.0.8103.79, headless; WebGL 2 through ANGLE on Metal, WebGPU
  adapter apple/metal-3; viewport 1400×900 at device pixel ratio 1; load average 3.3 at the start.
- Working tree of branch `renderer-v1` on `f19d855` (the renderer was not committed yet).
- Graph: `benchmarks/overview/generate.py --nodes 10000 --seed 1` (hierarchical, synthetic), imported with `limit`
  1000, 3000 and 10000 and laid out by the WASM engine in the page; the scene's sizes are in the table.
- Motion: 240 animation frames, a zoom from the whole drawing to 8 times in and back, with a pan on a circle. SVG is
  drawn once by `renderSvg` and then moved by a CSS transform per frame (the cheapest way to move an SVG drawing; the
  playground draws the SVG again on every zoom step). The far layer gets `setView` and `draw` per frame, with the texts
  on its 2D canvas.

Frame interval in ms (median / 95th percentile / maximum; frames over 33 ms), the first drawing in ms, and the CPU time
of `draw` (median):

| limit | boxes | routes | segments | SVG frames | SVG first | WebGPU frames | WebGPU first | WebGPU draw | WebGL 2 frames | WebGL 2 first | WebGL 2 draw |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 000 | 1 270 | 1 209 | 5 282 | 16.7 / 17.6 / 21.1; 0 | 46 | 16.7 / 17.5 / 18.0; 0 | 29 | 0.3 | 16.7 / 17.6 / 17.8; 0 | 49 | 0.1 |
| 3 000 | 3 438 | 4 021 | 20 022 | 16.7 / 17.5 / 26.4; 0 | 145 | 16.7 / 17.4 / 17.8; 0 | 43 | 0.3 | 16.7 / 17.4 / 17.7; 0 | 49 | 0.1 |
| 10 000 | 10 000 | 15 111 | 69 104 | 17.7 / 57.3 / 94.4; 85 | 451 | 16.7 / 17.3 / 56.2; 2 | 120 | 0.3 | 16.7 / 17.0 / 18.1; 0 | 83 | 0.1 |

Reading:

- Up to a few thousand elements SVG keeps the frame rate too when it is only moved; at 10 000 boxes and 69 000 segments
  a third of its frames take more than 33 ms. Both GPU backends keep the 60 Hz frame interval there; their CPU time per
  frame is below a millisecond, so the GPU and the compositor set the pace, not the drawing code.
- The first drawing (buffers built from the scene and uploaded, first frame) is 3 to 5 times faster than SVG's at
  10 000 boxes.
- One run per arm and size on a headless browser; the two slow WebGPU frames at 10 000 are single spikes. Not a claim
  about other GPUs, browsers or a display with a higher refresh rate; the budget and the thresholds of the switch (V2)
  are still to be set from such runs.
