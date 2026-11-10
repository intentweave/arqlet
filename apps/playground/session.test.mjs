import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {Session} from './session.js';

// The rules of the policy are cases in examples/fixtures/session/cases.json. crates/runtime/src/session.rs runs the same
// cases (test `shared_cases`), so the page's policy and the Rust policy cannot drift apart.
const {cases} = JSON.parse(readFileSync(new URL('../../examples/fixtures/session/cases.json', import.meta.url), 'utf8'));
const identities = {
  base: {graphRevision: 2, baseLayoutVersion: 3, optionsKey: 'k'},
  newer: {graphRevision: 3, baseLayoutVersion: 3, optionsKey: 'k'},
};

function run(session, step) {
  switch (step.do) {
    case 'start': {
      const r = session.start(step.job, identities[step.base]);
      return {refused: !!r.refused, superseded: r.superseded ?? null};
    }
    case 'offer':
      return {verdict: session.offer({jobId: step.job, sequence: step.sequence, input: identities[step.base], snapshot: {}}).verdict};
    case 'take':
      return {sequence: session.takePending()?.sequence ?? null};
    case 'edit':
      return {cancelled: session.edit(identities[step.base])};
    case 'finish': {
      const r = session.finish(step.job);
      return {outcome: r.outcome, improved: r.improved ?? null, resultSequence: r.result?.sequence ?? null};
    }
    case 'fail': {
      const r = session.fail(step.job);
      return {outcome: r.outcome, shown: r.shown ?? null};
    }
    case 'state':
      return {activeJob: session.activeJob, shown: session.shown};
    default:
      throw Error(`unknown step ${step.do}`);
  }
}

for (const c of cases) {
  test(c.name, () => {
    const session = new Session();
    c.steps.forEach((step, index) => assert.deepEqual(run(session, step), step.expect, `step ${index + 1}: ${step.do}`));
  });
}

test('the shared file has the cases the page relies on', () => {
  assert.ok(cases.length >= 9);
});
