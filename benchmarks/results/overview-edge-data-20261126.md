# Overview answer with edge data: size (#209)

Machine: Apple M3, 16 GB, macOS 26.0. Revision a471a11 (branch `overview-server-result`). Sizes only; no timing.

Graphs: `python3 benchmarks/overview/generate.py --nodes N --seed 1` (SHA-256 `2fe8e1c6…` for N = 10 000,
`2e32aa73…` for N = 100 000), with default overview options. The generator has one edge kind and no weights; for the
other rows the script draws six kinds, and weights 1 to 20, from a seeded generator (seed 1). Bytes are the answer as
`layout-command` writes it; gzip is zlib's default level.

    node benchmarks/overview/edge-data-size.mjs target/fast/layout-command \
      benchmarks/datasets/overview-10000.json benchmarks/datasets/overview-100000.json

| Nodes / edges | Variant | Without: bytes / gzip | Added by `edgeData`: bytes / gzip | As columns instead |
|---|---|---|---|---|
| 10 000 / 15 111 | one kind | 775 230 / 196 238 | 30 256 (3.9 %) / 320 (0.2 %) | 60 444 |
| 10 000 / 15 111 | six kinds | 775 230 / 196 238 | 30 305 (3.9 %) / 8 616 (4.4 %) | 60 444 |
| 10 000 / 15 111 | six kinds, weights | 780 576 / 204 563 | 68 802 (8.8 %) / 18 813 (9.2 %) | 120 888 |
| 100 000 / 150 923 | one kind | 8 579 588 / 2 010 259 | 301 880 (3.5 %) / 1 446 (0.1 %) | 603 692 |
| 100 000 / 150 923 | six kinds | 8 579 588 / 2 010 259 | 301 929 (3.5 %) / 66 240 (3.3 %) | 603 692 |
| 100 000 / 150 923 | six kinds, weights | 8 630 515 / 2 098 776 | 686 741 (8.0 %) / 174 389 (8.3 %) | 1 207 384 |

- In JSON a kind costs about 2 bytes per edge and a weight 2 to 3. Each column takes 4 bytes per edge (`u32`), so the
  columns are larger than the JSON digits here, as for `edges`.
- The "without" rows are the answer as before this change. The weighted rows' base differs because weights change the
  layout.
