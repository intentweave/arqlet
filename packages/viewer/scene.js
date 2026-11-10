/**
 * Scene v1 consumer helpers. The geometry is computed by the Rust scene crate (`build_scene`, operation
 * `scene`); this module only checks the schema and offers path helpers. It runs no layout and does not
 * change scene data.
 */
export function checkScene(scene) {
  if (scene.schemaVersion !== 1) throw Error('Unsupported scene schema');
  return scene;
}
export function routePath(points) { return points.map((p, i) => `${i ? 'L' : 'M'} ${p.x} ${p.y}`).join(' '); }
