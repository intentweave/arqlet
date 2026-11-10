// SVG pictures of the normalized drawings of `compare.mjs`, one page per dataset with the engines side by side.
// Every engine is drawn from the same geometry format (nodes with absolute boxes, edges as polylines), so the pictures show
// exactly what the metrics score. Containers are drawn first (outermost first), then the routes, then the leaves.
import {writeFileSync, mkdirSync} from 'node:fs';

const escape = text => String(text).replace(/[&<>"]/g, c => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;'})[c]);

// One drawing as an SVG element, scaled into `width` pixels.
export function drawingSvg(out, labels, width = 900) {
  const boxes = out.nodes.filter(n => Number.isFinite(n.x) && Number.isFinite(n.w));
  const xs = [...boxes.flatMap(n => [n.x, n.x + n.w]), ...out.edges.flatMap(e => e.points.map(p => p.x))];
  const ys = [...boxes.flatMap(n => [n.y, n.y + n.h]), ...out.edges.flatMap(e => e.points.map(p => p.y))];
  if (!xs.length) return '<svg width="10" height="10"></svg>';
  const pad = 20;
  const [x0, x1, y0, y1] = [Math.min(...xs) - pad, Math.max(...xs) + pad, Math.min(...ys) - pad, Math.max(...ys) + pad];
  const scale = width / (x1 - x0);
  const height = Math.max(40, Math.round((y1 - y0) * scale));
  const depth = new Map();
  const parent = new Map(out.nodes.map(n => [n.id, n.parentId]));
  const depthOf = id => {
    if (!depth.has(id)) depth.set(id, parent.get(id) ? depthOf(parent.get(id)) + 1 : 0);
    return depth.get(id);
  };
  const containers = boxes.filter(n => n.container).sort((a, b) => depthOf(a.id) - depthOf(b.id));
  const leaves = boxes.filter(n => !n.container);
  // Stroke widths and text sizes in drawing units, so that they stay readable at any scale.
  const unit = 1 / scale;
  const parts = [];
  for (const n of containers) {
    parts.push(`<rect x="${n.x}" y="${n.y}" width="${n.w}" height="${n.h}" rx="${4 * unit}" fill="var(--container)" stroke="var(--container-line)" stroke-width="${unit}"><title>${escape(labels.get(n.id) ?? n.id)}</title></rect>`);
    parts.push(`<text x="${n.x + 4 * unit}" y="${n.y + 11 * unit}" font-size="${9 * unit}" fill="var(--muted)">${escape(labels.get(n.id) ?? n.id)}</text>`);
  }
  for (const e of out.edges) {
    if (e.points.length < 2) continue;
    const d = e.points.map((p, i) => `${i ? 'L' : 'M'}${p.x},${p.y}`).join('');
    parts.push(`<path d="${d}" fill="none" stroke="var(--edge)" stroke-width="${unit}" marker-end="url(#arrow)"><title>${escape(e.id)}: ${escape(e.source)} → ${escape(e.target)}</title></path>`);
  }
  for (const n of leaves) {
    parts.push(`<rect x="${n.x}" y="${n.y}" width="${n.w}" height="${n.h}" rx="${3 * unit}" fill="var(--leaf)" stroke="var(--leaf-line)" stroke-width="${unit}"><title>${escape(labels.get(n.id) ?? n.id)}</title></rect>`);
  }
  return `<svg viewBox="${x0} ${y0} ${x1 - x0} ${y1 - y0}" width="${width}" height="${height}" xmlns="http://www.w3.org/2000/svg">
<defs><marker id="arrow" viewBox="0 0 10 10" refX="10" refY="5" markerWidth="${6}" markerHeight="${6}" markerUnits="userSpaceOnUse" orient="auto-start-reverse"><path d="M0,0L10,5L0,10z" fill="var(--edge)" transform="scale(${unit})"/></marker></defs>
${parts.join('\n')}
</svg>`;
}

// The page of one dataset: every engine's drawing with its scores, in the order given.
export function writeComparisonPage(dir, dataset, view, panels) {
  mkdirSync(dir, {recursive: true});
  const labels = new Map(view.nodes.map(n => [n.id, n.label ?? n.id]));
  const cards = panels.map(({engine, out, row}) => {
    const scores = row ? `crossings ${row.crossings} · bends ${row.bends} · length ${Math.round(row.length)} · area ${Math.round(row.area)} · line-node ${row.lineNodeHits} · ${row.layoutMs} ms` : '';
    return `<figure><figcaption><strong>${escape(engine)}</strong><span>${escape(scores)}</span></figcaption>${drawingSvg(out, labels)}</figure>`;
  });
  const html = `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>${escape(dataset)} comparison</title>
<style>
:root { --bg: #f7f8f9; --fg: #16191d; --muted: #66707a; --container: #eef1f4; --container-line: #a9b2bc; --leaf: #ffffff; --leaf-line: #4b5560; --edge: #1f5fd1; }
@media (prefers-color-scheme: dark) { :root:not([data-theme="light"]) { --bg: #17191c; --fg: #eef0f2; --muted: #9aa3ad; --container: #22262b; --container-line: #4a525c; --leaf: #333a42; --leaf-line: #c9d0d8; --edge: #6aa5ff; color-scheme: dark; } }
:root[data-theme="dark"] { --bg: #17191c; --fg: #eef0f2; --muted: #9aa3ad; --container: #22262b; --container-line: #4a525c; --leaf: #333a42; --leaf-line: #c9d0d8; --edge: #6aa5ff; color-scheme: dark; }
body { margin: 0; padding-block: 16px; padding-left: max(16px, env(safe-area-inset-left, 0px)); padding-right: max(16px, env(safe-area-inset-right, 0px)); background: var(--bg); color: var(--fg); font: 14px system-ui, sans-serif; }
main { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 560px), 1fr)); gap: 24px; }
figure { margin: 0; min-width: 0; }
figcaption { display: flex; flex-direction: column; gap: 2px; margin-bottom: 6px; }
figcaption span { color: var(--muted); font-variant-numeric: tabular-nums; }
svg { width: 100%; height: auto; border: 1px solid var(--container-line); border-radius: 6px; background: var(--bg); }
</style></head><body>
<h1>${escape(dataset)}</h1>
<p>${view.nodes.length} nodes, ${view.edges.filter(e => e.target !== null).length} edges. The same view for every engine; scores from one metric implementation (crates/scene).</p>
<main>
${cards.join('\n')}
</main></body></html>
`;
  writeFileSync(`${dir}/${dataset}.html`, html);
}
