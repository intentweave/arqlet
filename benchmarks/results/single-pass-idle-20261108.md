# Single pass: the idle limit of the hierarchical sweep again, 2026-11-08

Revision `df25eea` (A3), Apple M3, release builds, single pass only, five seeds (`--seeds 5`), metric means. Load average 6–13
from other work during the runs: the times below are an indication, not a result.

Raw: [single-pass-idle-20261108.json](single-pass-idle-20261108.json). Commands: `node benchmarks/eval/single-pass.mjs --corpus
--raw --repeat 1 --seeds 5 --arms singlePass --no-elk` (11 mixed datasets), the same without `--raw` (31 uniform views) and
without `--corpus` (frozen views), with `IDLE_SWEEPS_SINGLE_PASS` at 1 and at 4.

## Why again

The idle limit (sweeps without improvement that end a run of the hierarchical sweep) was set to 1 for single pass on 2026-11-06,
because it drew 4.3 % fewer crossings than 4 on the uniform views then ([levers](single-pass-levers-20261106.md)). On uml-class
the five-seed mean was 8.6 crossings against the default's 8, with seeds between 7 and 11; with four idle sweeps it was 6.6.

## Mixed datasets (against the A3 report, idle 1)

| Variant | vs default | crossings | bends |
|---|---|---:|---:|
| runs 5, idle 1 (before) | 10 better / 1 worse | 3861.6 | 3422 |
| **runs 5, idle 4** | **11 better / 0 worse** | **3854.0** | 3431 |
| runs 8, idle 4 | 11 / 0 | 3899.2 | 3454 |
| runs 8, idle 1 | 11 / 0 | 3896.6 | 3450 |

Runs 5, idle 4 against idle 1, per view: 5 better (uml-class 8.6 → 6.6, workflow-dag 546.0 → 542.2, bpmn-pools 14.6 → 13.4,
deployment-cluster 113.0 → 112.4, network-vlan 43.6 → 43.4), 5 equal, 1 worse (cloud-architecture 30.4 → 30.6, still below the
default's 31).

## Uniform and frozen views

| Five-seed sums | idle 1 | idle 4 |
|---|---:|---:|
| uniform crossings (31 views) | 9092.8 | **8566.4** |
| uniform bends | 6206.6 | 6303.8 |
| frozen crossings (3 views) | 3943.8 | **3848.4** |
| frozen bends | 2025.2 | 2027.6 |

Uniform, per view: 6 better (layerweave-code 6332 → 5751 crossings, org-chart 31.0 → 26.4, deployment-cluster 92.6 → 85.8,
activity-order, network-vlan, mixed-large by a shared line pair), 22 equal, 3 worse (synthetic-2k 1731 → 1791 crossings,
workflow-dag 231.4 → 235.4, layerweave-pipeline 6.2 → 6.6). Frozen: code view and org-chart better, workflow-dag worse.

Import time, sum over the uniform views (indication only, load 8–10): 465 → 492 ms; frozen: 99 → 100 ms.

## Decision

`IDLE_SWEEPS_SINGLE_PASS` is now `IDLE_SWEEPS` (4); the number of runs stays 5. The default pipeline is unchanged
(byte-identical on all 31 datasets at threads 1 and 4). The time cost needs a measurement on a quiet machine.
