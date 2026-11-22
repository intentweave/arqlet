# ADR 0026: Appearance: one style for every renderer

Date: 2026-11-22.
Status: accepted (provisional API: `@arqlet/viewer/style.js`).

Related: [ADR 0008](0008-engine-client-viewer-boundaries.md) (the viewer paints accepted geometry; hosts may draw their
own node content), [ADR 0025](0025-scene-renderer-webgl-far-svg-near.md) (SVG, the GPU far layer and React Flow draw
one scene).

## Context

Three renderers draw one scene: `renderSvg`, the GPU far layer (WebGPU or WebGL 2) and a React Flow host. Their colours
were written into each of them, the same values three times; no renderer used an element's `kind` (a call and an import
edge, a module and a function looked alike), and a host could not set a theme of its own.

How others do it:

- **CSS on the elements** (React Flow's CSS variables and class names, plain SVG with a stylesheet): natural for DOM and
  SVG, but a GPU renderer cannot read CSS per element; reading computed styles of thousands of elements is far too slow,
  and there is no DOM element from afar.
- **A stylesheet of selectors over the data** (Cytoscape.js: `node[kind = "module"] { background-color: … }`, mapped
  values, categorical colours): one declarative description, resolved per element in JavaScript, independent of the
  renderer.
- **Callbacks per element** (sigma.js reducers, deck.gl accessors, yFiles style objects): flexible but code, not data:
  it cannot travel in a snapshot, a configuration or a backend answer.
- **Design tokens** (named colours for parts and states, set by a theme, often from CSS custom properties): how
  component libraries keep dark, light and brand themes in one place.

Libraries: Cytoscape's style engine is not usable without Cytoscape; theme libraries (CSS-in-JS, token tools) style
DOM, not GPU buffers; colour libraries (culori, chroma.js) solve only the parsing of colours. None fits a renderer that
must hand colours to SVG attributes, GPU vertex buffers and React props alike.

## Decision

One small style model in `@arqlet/viewer` (`style.js`), pure and serialisable, read by every renderer:

1. **Theme (tokens).** Named colours for every part (frames, leaves, text, routes, arrowheads, bus bars, labels, ports,
   controls) and every state (selected, invalid, active route, trace, hover, pins). The defaults are the colours drawn
   before, so a scene without a style draws exactly as before (the SVG golden files are unchanged). A page may set any
   token as a CSS custom property (`--arqlet-leaf-fill`, `--arqlet-route`, …; `themeFromCss`): CSS reaches the GPU
   layer through the theme, read once, not per element.
2. **Rules.** `[{selector, style}]` with selectors over an element's data: `node`, `container`, `leaf`, `edge` or `*`,
   then `[key=value]` (any data field: `kind`, `shape`, …) and `#id`. Later rules win, as in CSS. Node properties:
   `fill`, `stroke`, `strokeWidth`, `text`, `dash`; edge properties: `stroke`, `arrow`, `width`, `opacity`, `dash`. A
   colour is a CSS colour, a token (`$route`) or `auto`: a categorical colour from the element's kind, the same for a
   kind in every view (a hash of the kind into a fixed palette).
3. **Rules decide by what an element is; states take the theme's colours** in every renderer (selection, trace, hover
   are not selectors). Keeps rules small and the states consistent.
4. **Every renderer reads the same resolved style.** `renderSvg(…, {style})`; the GPU buffers (`sceneBuffers(…, {style})`,
   CSS colours parsed to RGBA once per style); `toReactFlow(…, {style})` puts each element's resolved look in `data.look`
   for the host's node and edge types; the scene view hands one style to both layers (`setState({style})`).
5. Built-in sheets: `default`, `byKind` (routes, arrowheads and leaf outlines by kind), `light`.

Checked: the SVG golden files unchanged; Node tests that one sheet gives the same colours in `renderSvg`, the GPU
buffers and React Flow; in Chrome (`browser-checks/scene-gl.mjs`) `byKind`, `light` and a sheet with tokens, widths,
opacity, dashes and `auto` fills, drawn by `renderSvg` and both GPU backends, differ in under 0.1 % of the pixels.

## Consequences

- A host themes the drawing with data (a sheet) or with CSS (tokens), and the far layer, the SVG near layer and React
  Flow stay alike.
- Not yet: rules by numeric ranges or mapped values (a width from `weight`), per-element text sizes and fonts, hull
  colours (still one hue per dimension), a style that travels in the snapshot or the configuration (a sheet is JSON,
  so this is possible later), and the GPU layer draws a dashed outline with its one dash pattern whatever `dash` says.
- The cluster overview (ADR 0018) has its own colours and does not read the style yet.
