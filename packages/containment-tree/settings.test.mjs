import test from 'node:test';
import assert from 'node:assert/strict';
import {chosenPhrase, describeSetting, formValueOf, formatValue, originOf, rootSettingsOf, settingsOf, sourceOf} from './settings.js';

const source = {
  defaultDirection: 'lr',
  defaultStrategy: 'none',
  defaultRouting: 'orthogonal',
  containerDirections: {outer: 'tb'},
  containerStrategies: {inner: ['tree', 'radial']},
  containerRouting: {},
  nodes: [
    {id: 'outer', parentId: null},
    {id: 'inner', parentId: 'outer'},
    {id: 'leaf', parentId: 'inner'},
  ],
};

test('a container takes a value from its nearest ancestor that sets it, and names it', () => {
  const inner = settingsOf(source, 'inner');
  assert.deepEqual(inner.direction.effective, {value: 'tb', owner: 'outer'});
  assert.equal(inner.direction.own, undefined);
  assert.equal(describeSetting('direction', inner.direction), 'Direction: TB · from outer');
});

test('a value set on the container itself is its own, and says so', () => {
  const outer = settingsOf(source, 'outer');
  assert.equal(outer.direction.own, 'tb');
  assert.equal(describeSetting('direction', outer.direction), 'Direction: TB · from outer · set here');
});

test('without any setting on the way up, the root default applies and no owner is named', () => {
  const leaf = settingsOf(source, 'leaf');
  assert.deepEqual(leaf.routing.effective, {value: 'orthogonal', owner: null});
  assert.equal(describeSetting('routing', leaf.routing), 'Routing: ORTHOGONAL');
});

test('an allowed set is shown as allowed, not edited as a single choice', () => {
  const inner = settingsOf(source, 'inner');
  assert.equal(formValueOf(inner.strategy.own), 'allowed');
  assert.equal(formatValue(inner.strategy.own), 'ALLOWED TREE / RADIAL');
  assert.equal(formValueOf(undefined), 'inherit');
  assert.equal(formValueOf('lr'), 'lr');
});

test('a value of auto reads as free, as in the inspector', () => {
  assert.equal(formatValue('auto'), 'FREE');
});

test('the source of a setting is set here, from an ancestor, or the default', () => {
  assert.equal(sourceOf(settingsOf(source, 'outer').direction), 'set here');
  assert.equal(sourceOf(settingsOf(source, 'inner').direction), 'from outer');
  assert.equal(sourceOf(settingsOf(source, 'leaf').routing), 'default');
});

// How a value was reached: fixed here, auto here (the engine chooses), an allowed set, inherited, or nobody's (the default).
test('the origin tells fixed from auto from inherited from the default', () => {
  const graph = {
    defaultDirection: 'lr',
    containerDirections: {fixed: 'tb', free: 'auto', some: ['lr', 'tb']},
    containerStrategies: {},
    nodes: [{id: 'fixed'}, {id: 'free'}, {id: 'some'}, {id: 'child', parentId: 'free'}, {id: 'plain'}],
  };
  const origin = (id, field) => originOf(settingsOf(graph, id)[field]);
  assert.equal(origin('fixed', 'direction'), 'fixed');
  assert.equal(origin('free', 'direction'), 'auto');
  assert.equal(origin('some', 'direction'), 'allowed');
  assert.equal(origin('child', 'direction'), 'inherited');
  assert.equal(origin('plain', 'strategy'), 'default');
  assert.equal(originOf(rootSettingsOf(graph).direction), 'fixed');
});

test('what a free direction chose is named apart from the setting, and only for an automatic value', () => {
  const graph = {
    defaultDirection: 'lr',
    containerDirections: {free: 'auto', fixed: 'tb'},
    nodes: [{id: 'free'}, {id: 'fixed'}, {id: 'child', parentId: 'free'}],
  };
  const orientations = {free: 'tb', fixed: 'lr', child: 'lr'};
  const free = settingsOf(graph, 'free', undefined, {orientations}).direction;
  assert.equal(free.chosen, 'tb');
  assert.equal(chosenPhrase(free), 'chose tb');
  // A fixed value chooses nothing, even if an orientation is known for the area.
  assert.equal(chosenPhrase(settingsOf(graph, 'fixed', undefined, {orientations}).direction), '');
  // Without the engine's report there is nothing to name.
  assert.equal(chosenPhrase(settingsOf(graph, 'free').direction), '');
  assert.equal(rootSettingsOf({defaultDirection: 'auto'}, {orientations: {'<root>': 'lr'}}).direction.chosen, 'lr');
});

test('what the search chose for a free strategy is named, a fixed one chooses nothing', () => {
  const graph = {defaultStrategy: 'auto', containerStrategies: {fixed: 'organic'}, nodes: [{id: 'fixed'}, {id: 'free'}]};
  const strategies = {'<root>': 'none', free: 'organic', fixed: 'organic'};
  assert.equal(chosenPhrase(settingsOf(graph, 'free', undefined, {strategies}).strategy), 'chose organic');
  assert.equal(chosenPhrase(settingsOf(graph, 'fixed', undefined, {strategies}).strategy), '');
  assert.equal(chosenPhrase(rootSettingsOf(graph, {strategies}).strategy), 'chose none');
  assert.equal(chosenPhrase(settingsOf(graph, 'free').strategy), '');
});
