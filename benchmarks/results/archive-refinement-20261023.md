# Archive refinement (opt-in), corpus run (2026-10-23)

Option `archiveRefinement: N` (default 0): after the first stage, the N archive entries after the incumbent each get one routing pass, in archive order, whether or not they were published. The pass winner competes under the shared key. Raw rows: `archive-refinement-{default,1,3}-20261023.jsonl`.

Acceptance criterion (an unpublished runner-up wins after limited refinement): met on org-chart (test `an_archive_runner_up_can_win_after_one_routing_pass`), and also on several corpus views (`strategy` starts with `routed-`).

By the accepted key (the decisive measure), N = 1 against default: 6 views better, 12 equal, 2 worse; N = 3: 7 better, 11 equal, 2 worse. The worse views are synthetic-2k 100 and intentweave-export 100. The key compares shared-route pairs before crossings, so crossing sums alone mislead: several views with more routed crossings are better by key.

Likely cause of the two worse views (not confirmed): path dependence. The pass can accept a winner that starts the global rounds from a different point, and those rounds end somewhere worse than the default path. The key is monotone within one run: a result never falls below its input.

Cost: the passes are routing rounds on the archive, budgeted as jobs. Refinement time rose about 7 percent on the 20 views with the archive kept (a single timing, not a benchmark claim); the passes add more.

Default output: unchanged (17 stable views identical).
