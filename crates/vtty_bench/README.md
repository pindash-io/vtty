# vtty_bench

Criterion throughput benchmarks: `vtty` (scalar and SIMD) vs `vte`, `vt100`,
`vtparse`, `rio-vt`, and `fux-vt`.

```bash
cargo bench -p vtty_bench
```

## Method

`vt_input` feeds a 2 MiB corpus through a counting sink (no grid mutation)
and reports median throughput, isolating parser + dispatch cost from screen
updates.

`vt_screen` runs the same parser through the real `vtty_screen` cell grid
(SGR apply + cell writes + 80×24 render), so it measures the full parse +
screen-update pipeline rather than the parser floor.

Corpora: `plain`, `scroll_storm`, `utf8_mixed`, `ansi_mixed`,
`ansi_two_styles`, `ansi_bare_csi`, `sgr_churn`, `cursor_motion`, `sync_churn`,
`truecolor_unique`, `bracketed_paste`, `osc_heavy`, `dcs_sixel`.

## Results — `vt_input` (parser floor)

| case | vtty | vtty-simd | vte | vtparse | rio-vt | fux-vt | vt100* |
| --- | --- | --- | --- | --- | --- | --- | --- |
| plain | 11.18 GiB/s | **25.52 GiB/s** | 2.55 GiB/s | 942 MiB/s | 3.33 GiB/s | 1.69 GiB/s | 183 MiB/s |
| scroll_storm | 2.52 GiB/s | 2.52 GiB/s | **2.56 GiB/s** | 970 MiB/s | 2.21 GiB/s | 490 MiB/s | 129 MiB/s |
| utf8_mixed | 1.18 GiB/s | **2.25 GiB/s** | 1.53 GiB/s | 535 MiB/s | 2.02 GiB/s | 412 MiB/s | 254 MiB/s |
| ansi_mixed | 1.59 GiB/s | **1.60 GiB/s** | 559 MiB/s | 375 MiB/s | 734 MiB/s | 529 MiB/s | 319 MiB/s |
| ansi_two_styles | 1.50 GiB/s | **1.52 GiB/s** | 665 MiB/s | 409 MiB/s | 670 MiB/s | 391 MiB/s | 273 MiB/s |
| ansi_bare_csi | **1.94 GiB/s** | 1.93 GiB/s | 980 MiB/s | 539 MiB/s | 846 MiB/s | 500 MiB/s | 242 MiB/s |
| sgr_churn | **1.81 GiB/s** | 1.80 GiB/s | 585 MiB/s | 427 MiB/s | 872 MiB/s | 587 MiB/s | 464 MiB/s |
| cursor_motion | 2.13 GiB/s | **2.14 GiB/s** | 617 MiB/s | 410 MiB/s | 776 MiB/s | 617 MiB/s | 464 MiB/s |
| sync_churn | **1.99 GiB/s** | 1.80 GiB/s | 674 MiB/s | 461 MiB/s | 895 MiB/s | 501 MiB/s | 153 MiB/s |
| truecolor_unique | **1.84 GiB/s** | 1.63 GiB/s | 599 MiB/s | 422 MiB/s | 941 MiB/s | 585 MiB/s | 520 MiB/s |
| bracketed_paste | **2.95 GiB/s** | 2.78 GiB/s | 1.37 GiB/s | 664 MiB/s | 1.42 GiB/s | 746 MiB/s | 217 MiB/s |
| osc_heavy | 2.88 GiB/s | **2.91 GiB/s** | 659 MiB/s | 256 MiB/s | 2.67 GiB/s | 2.13 GiB/s | 252 MiB/s |
| dcs_sixel | 7.91 GiB/s | **11.81 GiB/s** | 1.02 GiB/s | 974 MiB/s | 7.35 GiB/s | 4.62 GiB/s | 783 MiB/s |

*vt100 is a full-screen emulator, not a standalone parser.

## Results — `vt_screen` (parse + cell grid)

| case | vtty | vtty-simd |
| --- | --- | --- |
| plain | 223 MiB/s | **226 MiB/s** |
| sgr_nvim | **572 MiB/s** | 565 MiB/s |
| sgr_churn | 854 MiB/s | **859 MiB/s** |
| truecolor_unique | **682 MiB/s** | 656 MiB/s |
| ansi_mixed | **547 MiB/s** | 546 MiB/s |

Measured on Apple Silicon (aarch64). Numbers are medians of 20 Criterion
samples from a full-suite run; throughput is sensitive to thermal throttling
and CPU clock, so results vary run-to-run — treat as relative rankings rather
than absolute figures.
