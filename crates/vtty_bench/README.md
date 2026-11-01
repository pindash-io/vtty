# vtty_bench

Criterion throughput benchmarks: `vtty` (scalar and SIMD) vs `vte`, `vt100`,
`vtparse`, and `rio-vt`.

```bash
cargo bench -p vtty_bench
```

## Method

Each benchmark feeds a 2 MiB corpus through a counting sink (no grid
mutation) and reports median throughput, isolating parser + dispatch cost
from screen updates.

Corpora: `plain`, `scroll_storm`, `utf8_mixed`, `ansi_mixed`,
`ansi_two_styles`, `ansi_bare_csi`, `sgr_churn`, `cursor_motion`, `sync_churn`,
`truecolor_unique`, `bracketed_paste`, `osc_heavy`, `dcs_sixel`.

## Results

| case | vtty | vtty-simd | vte | vtparse | rio-vt | vt100* |
| --- | --- | --- | --- | --- | --- | --- |
| plain | 10.47 GiB/s | **27.79 GiB/s** | 2.01 GiB/s | 695 MiB/s | 1.77 GiB/s | 149 MiB/s |
| scroll_storm | 2.14 GiB/s | **2.36 GiB/s** | 2.00 GiB/s | 721 MiB/s | 1.93 GiB/s | 104 MiB/s |
| utf8_mixed | 1.22 GiB/s | **2.12 GiB/s** | 1.27 GiB/s | 390 MiB/s | 1.77 GiB/s | 202 MiB/s |
| ansi_mixed | **1.32 GiB/s** | 1.27 GiB/s | 473 MiB/s | 311 MiB/s | 560 MiB/s | 251 MiB/s |
| ansi_two_styles | **1.31 GiB/s** | 1.30 GiB/s | 532 MiB/s | 331 MiB/s | 552 MiB/s | 216 MiB/s |
| ansi_bare_csi | **1.78 GiB/s** | 1.58 GiB/s | 709 MiB/s | 414 MiB/s | 690 MiB/s | 191 MiB/s |
| sgr_churn | 1.25 GiB/s | **1.25 GiB/s** | 492 MiB/s | 348 MiB/s | 674 MiB/s | 360 MiB/s |
| cursor_motion | **1.21 GiB/s** | 1.21 GiB/s | 496 MiB/s | 339 MiB/s | 580 MiB/s | 368 MiB/s |
| sync_churn | **1.71 GiB/s** | 1.52 GiB/s | 561 MiB/s | 366 MiB/s | 678 MiB/s | 127 MiB/s |
| truecolor_unique | **1.13 GiB/s** | 1.06 GiB/s | 499 MiB/s | 331 MiB/s | 692 MiB/s | 399 MiB/s |
| bracketed_paste | **2.82 GiB/s** | 2.14 GiB/s | 1.08 GiB/s | 498 MiB/s | 1.03 GiB/s | 176 MiB/s |
| osc_heavy | **2.59 GiB/s** | 2.54 GiB/s | 533 MiB/s | 204 MiB/s | 2.11 GiB/s | 186 MiB/s |
| dcs_sixel | 6.54 GiB/s | **6.71 GiB/s** | 894 MiB/s | 805 MiB/s | 5.91 GiB/s | 663 MiB/s |

*vt100 is a full-screen emulator, not a standalone parser.

Measured on Apple Silicon (aarch64). Numbers are medians of 20 Criterion
samples from a full-suite run; throughput is sensitive to thermal throttling
and CPU clock, so results vary run-to-run — treat as relative rankings rather
than absolute figures.
