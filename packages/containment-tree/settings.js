/**
 * The layout settings of a container, with their provenance (ADR 0010, step 3): the value the container sets itself,
 * and the value in force, with the ancestor it comes from. The rules are those of the inspector: the nearest container
 * on the way up that sets the value wins, and the root supplies the default. Pure and framework-independent.
 */

/** The fields of a container's layout settings and the operation that changes each one. */
export const SETTING_FIELDS = {
  direction: {map: 'containerDirections', root: 'defaultDirection', choices: ['lr', 'rl', 'tb', 'bt'], name: 'Direction'},
  strategy: {map: 'containerStrategies', root: 'defaultStrategy', choices: ['none', 'tree', 'radial', 'organic'], name: 'Strategy'},
  routing: {map: 'containerRouting', root: 'defaultRouting', choices: ['orthogonal', 'roundedOrthogonal', 'polyline', 'roundedPolyline', 'straight'], name: 'Routing'},
};

/**
 * The value the engine uses at the root when the graph declares none: direction left to right, strategy none, and no
 * declared routing (the layout's orthogonal fallback). Only routing can be cleared at the root.
 */
export const ROOT_DEFAULTS = {direction: 'lr', strategy: 'none', routing: undefined};

/**
 * The layout settings of the whole view (the root), in the same shape as `settingsOf`: `own` is the declared value or
 * undefined, and `effective` the value in force.
 */
export function rootSettingsOf(source, {orientations = {}, strategies = {}} = {}) {
  const result = {};
  for (const [field, spec] of Object.entries(SETTING_FIELDS)) {
    const own = source[spec.root] ?? undefined;
    result[field] = {
      own,
      effective: {value: own ?? ROOT_DEFAULTS[field], owner: null},
      name: spec.name,
      choices: spec.choices,
      root: true,
      clearable: field === 'routing',
      // What a free direction chose for the view's own arrangement.
      chosen: field === 'direction' ? orientations['<root>'] : field === 'strategy' ? strategies['<root>'] : undefined,
    };
  }
  return result;
}

/**
 * The settings of one container in `source` (the source graph of the snapshot). `parentOf` maps a node ID to its
 * parent's ID or null; it is built from the source's nodes when omitted.
 */
export function settingsOf(source, id, parentOf = parentMapOf(source), {orientations = {}, strategies = {}} = {}) {
  const result = {};
  for (const [field, spec] of Object.entries(SETTING_FIELDS)) {
    const map = source[spec.map] ?? {};
    const own = map[id];
    result[field] = {
      own,
      effective: effectiveOf(map, source[spec.root], id, parentOf),
      name: spec.name,
      choices: spec.choices,
      // What a free direction chose for this container's own arrangement (the engine reports it; absent when it did not choose).
      chosen: field === 'direction' ? orientations[id] : field === 'strategy' ? strategies[id] : undefined,
    };
  }
  return result;
}

/** The value a field takes in the form: `inherit`, one fixed choice, or `allowed` for a set the form does not edit. */
export function formValueOf(own) {
  if (own === undefined) return 'inherit';
  if (Array.isArray(own)) return 'allowed';
  return own;
}

/** How a value reads in the text of a field, as in the inspector. */
export function formatValue(value) {
  if (typeof value === 'string') return value === 'auto' ? 'FREE' : value.toUpperCase();
  if (Array.isArray(value)) return `ALLOWED ${value.map(x => x.toUpperCase()).join(' / ')}`;
  return 'NONE';
}

/** Where a setting comes from, as the settings editor shows it: set here, from an ancestor, or the default. */
export function sourceOf(setting) {
  if (setting.own !== undefined) return 'set here';
  if (setting.effective.owner) return `from ${setting.effective.owner}`;
  return 'default';
}

/**
 * How a setting got its value, which the source alone does not say: `fixed` (one value set here), `auto` (set to auto here: the
 * engine chooses), `allowed` (a set set here), `inherited` (set by an ancestor) or `default` (nobody sets it, the engine's own).
 */
export function originOf(setting) {
  const own = setting.own;
  if (own === undefined) return setting.effective.owner ? 'inherited' : 'default';
  if (Array.isArray(own)) return 'allowed';
  return own === 'auto' ? 'auto' : 'fixed';
}

/** What a free choice came to, as a phrase: `chose tb`, or nothing when the setting is fixed or the engine did not choose. */
export function chosenPhrase(setting) {
  const value = setting.effective.value;
  return (value === 'auto' || Array.isArray(value)) && setting.chosen ? `chose ${setting.chosen}` : '';
}

/** The text that names a field: its value in force, the ancestor it comes from, and whether the container sets it. */
export function describeSetting(field, setting) {
  const from = setting.effective.owner ? ` · from ${setting.effective.owner}` : '';
  const here = setting.own !== undefined ? ' · set here' : '';
  return `${setting.name}: ${formatValue(setting.effective.value)}${from}${here}`;
}

function effectiveOf(map, root, id, parentOf) {
  let cursor = id;
  while (cursor !== null && cursor !== undefined) {
    if (map[cursor] !== undefined) return {value: map[cursor], owner: cursor};
    cursor = parentOf.get(cursor) ?? null;
  }
  return {value: root, owner: null};
}

function parentMapOf(source) {
  return new Map((source.nodes ?? []).map(node => [node.id, node.parentId ?? null]));
}
