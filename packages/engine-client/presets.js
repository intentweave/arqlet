// Preset logic shared by the SVG demo and the React Flow reference. Pure: no DOM, no fetch.
// A preset is active only if the snapshot names this exact version; an older version is reported as stale.

// Which preset a snapshot is in: `preset` only for the exact `<id>@<version>` currently shipped.
export function presetState(snapshot, presets) {
  const name = snapshot?.options?.configId;
  if (!name) return {preset: null, stale: false};
  const [id, version] = name.split('@');
  const known = presets.get(id) ?? null;
  if (!known) return {preset: null, stale: false};
  const current = String(known.version) === version;
  return {preset: current ? known : null, stale: !current};
}

// Applies a preset as two engine operations: the direction, then the config set. The second step starts
// only if the first succeeded and the view is still the one the sequence began with (`stillCurrent`).
// `run(request)` resolves to {ok, snapshot?, error?}. Nothing is retried, and no refinement runs in between.
export async function applyPreset({preset, snapshot, run, stillCurrent}) {
  const failed = reason => ({ok: false, error: `Preset not applied: ${reason}`});
  let current = snapshot;
  // `hybrid` was the compact choice per area, which a free direction is now: the preset sets `auto`.
  const wanted = preset.graph.defaultDirection === 'hybrid' ? 'auto' : preset.graph.defaultDirection;
  if ((current.sourceGraph ?? current.graph).defaultDirection !== wanted) {
    const step = await run({op: 'direction', snapshot: current, nodeId: null, direction: wanted});
    if (!step.ok) return failed(step.error ?? 'Direction');
    if (!stillCurrent()) return failed('The view changed in the meantime.');
    current = step.snapshot;
  }
  const step = await run({op: 'configure', snapshot: current, options: {configSet: preset.configSet}});
  if (!step.ok) return failed(step.error ?? 'Configuration');
  if (!stillCurrent()) return failed('The view changed in the meantime.');
  return {ok: true, snapshot: step.snapshot};
}
