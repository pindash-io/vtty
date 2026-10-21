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
| plain | 12.43 GiB/s | **28.18 GiB/s** | 2.03 GiB/s | 706 MiB/s | 1.79 GiB/s | 212 MiB/s |
| scroll_storm | 2.16 GiB/s | **2.39 GiB/s** | 2.03 GiB/s | 733 MiB/s | 1.94 GiB/s | 116 MiB/s |
| utf8_mixed | 1.24 GiB/s | **2.15 GiB/s** | 1.31 GiB/s | 394 MiB/s | 1.79 GiB/s | 272 MiB/s |
| ansi_mixed | 1.45 GiB/s | **1.46 GiB/s** | 484 MiB/s | 314 MiB/s | 581 MiB/s | 280 MiB/s |
| ansi_two_styles | **1.47 GiB/s** | 1.43 GiB/s | 540 MiB/s | 341 MiB/s | 556 MiB/s | 243 MiB/s |
| ansi_bare_csi | **1.74 GiB/s** | 1.65 GiB/s | 713 MiB/s | 421 MiB/s | 710 MiB/s | 239 MiB/s |
| sgr_churn | 1.44 GiB/s | **1.53 GiB/s** | 502 MiB/s | 364 MiB/s | 686 MiB/s | 389 MiB/s |
| cursor_motion | 1.42 GiB/s | **1.43 GiB/s** | 503 MiB/s | 349 MiB/s | 594 MiB/s | 387 MiB/s |
| sync_churn | **1.86 GiB/s** | 1.66 GiB/s | 579 MiB/s | 380 MiB/s | 700 MiB/s | 144 MiB/s |
| truecolor_unique | **1.43 GiB/s** | 1.32 GiB/s | 510 MiB/s | 346 MiB/s | 716 MiB/s | 420 MiB/s |
| bracketed_paste | **2.74 GiB/s** | 2.24 GiB/s | 1.12 GiB/s | 511 MiB/s | 1.06 GiB/s | 237 MiB/s |
| osc_heavy | 2.67 GiB/s | **2.71 GiB/s** | 541 MiB/s | 208 MiB/s | 2.31 GiB/s | 215 MiB/s |
| dcs_sixel | 6.72 GiB/s | **7.21 GiB/s** | 906 MiB/s | 813 MiB/s | 6.04 GiB/s | 737 MiB/s |

*vt100 is a full-screen emulator, not a standalone parser.

Measured on Apple Silicon (aarch64). Numbers are medians of 20 Criterion
samples from a full-suite run; throughput is sensitive to thermal throttling
and CPU clock, so results vary run-to-run — treat as relative rankings rather
than absolute figures.
