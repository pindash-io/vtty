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
| plain | 4.16 GiB/s | **6.39 GiB/s** | 848 MiB/s | 295 MiB/s | 735 MiB/s | 71 MiB/s |
| scroll_storm | 767 MiB/s | **967 MiB/s** | 607 MiB/s | 276 MiB/s | 817 MiB/s | 47 MiB/s |
| utf8_mixed | 686 MiB/s | **825 MiB/s** | 455 MiB/s | 166 MiB/s | 792 MiB/s | 112 MiB/s |
| ansi_mixed | 606 MiB/s | **667 MiB/s** | 169 MiB/s | 125 MiB/s | 231 MiB/s | 117 MiB/s |
| ansi_two_styles | 652 MiB/s | **729 MiB/s** | 242 MiB/s | 140 MiB/s | 237 MiB/s | 105 MiB/s |
| ansi_bare_csi | **1000 MiB/s** | 831 MiB/s | 299 MiB/s | 162 MiB/s | 251 MiB/s | 94 MiB/s |
| sgr_churn | **790 MiB/s** | 700 MiB/s | 186 MiB/s | 144 MiB/s | 299 MiB/s | 165 MiB/s |
| cursor_motion | **708 MiB/s** | 676 MiB/s | 240 MiB/s | 146 MiB/s | 261 MiB/s | 151 MiB/s |
| sync_churn | **1.02 GiB/s** | 811 MiB/s | 227 MiB/s | 167 MiB/s | 281 MiB/s | 56 MiB/s |
| truecolor_unique | **715 MiB/s** | 499 MiB/s | 204 MiB/s | 134 MiB/s | 272 MiB/s | 163 MiB/s |
| bracketed_paste | **1.17 GiB/s** | 1.07 GiB/s | 418 MiB/s | 181 MiB/s | 442 MiB/s | 83 MiB/s |
| osc_heavy | 1.07 GiB/s | **1.14 GiB/s** | 238 MiB/s | 94 MiB/s | 632 MiB/s | 85 MiB/s |
| dcs_sixel | 2.52 GiB/s | 2.91 GiB/s | 391 MiB/s | 318 MiB/s | **3.03 GiB/s** | 294 MiB/s |

*vt100 is a full-screen emulator, not a standalone parser.

Measured on Apple Silicon (aarch64). Numbers are medians of 20 Criterion
samples from a full-suite run; throughput is sensitive to thermal throttling,
so a full suite heats the CPU and depresses later cases (a fresh single-case
run of `plain` measures ~2× higher on this hardware).
