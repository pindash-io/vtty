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

`csi_units` runs one repeated sequence at a time (~40 s per run, `--bench
csi_units`), so a change to a fast path can be checked in seconds. `vt_bandwidth`
re-runs a subset with a working set past the last-level cache, and `vt_parallel`
shards it across threads.

Corpora: `plain`, `scroll_storm`, `utf8_mixed`, `ansi_mixed`,
`ansi_two_styles`, `ansi_bare_csi`, `sgr_churn`, `cursor_motion`, `sync_churn`,
`truecolor_unique`, `sgr_nvim`, `bracketed_paste`, `osc_heavy`, `dcs_sixel`,
`tui_redraw`, `cli_color`.

## Results: `vt_input` (parser floor)

| case | vtty | vtty-simd | vte | vtparse | rio-vt | fux-vt | vt100* |
| --- | --- | --- | --- | --- | --- | --- | --- |
| plain | 10.46 GiB/s | **19.66 GiB/s** | 2.66 GiB/s | 944 MiB/s | 3.25 GiB/s | 1.76 GiB/s | 183 MiB/s |
| scroll_storm | 2.79 GiB/s | **2.79 GiB/s** | 2.71 GiB/s | 967 MiB/s | 2.22 GiB/s | 498 MiB/s | 130 MiB/s |
| utf8_mixed | 1.45 GiB/s | **2.25 GiB/s** | 1.56 GiB/s | 531 MiB/s | 1.99 GiB/s | 415 MiB/s | 256 MiB/s |
| ansi_mixed | 1.96 GiB/s | **1.98 GiB/s** | 592 MiB/s | 368 MiB/s | 733 MiB/s | 538 MiB/s | 321 MiB/s |
| ansi_two_styles | **2.22 GiB/s** | 2.22 GiB/s | 713 MiB/s | 393 MiB/s | 716 MiB/s | 396 MiB/s | 274 MiB/s |
| ansi_bare_csi | 2.16 GiB/s | **2.21 GiB/s** | 946 MiB/s | 529 MiB/s | 866 MiB/s | 509 MiB/s | 242 MiB/s |
| sgr_churn | **1.90 GiB/s** | 1.90 GiB/s | 641 MiB/s | 425 MiB/s | 874 MiB/s | 600 MiB/s | 459 MiB/s |
| cursor_motion | **2.03 GiB/s** | 2.02 GiB/s | 671 MiB/s | 401 MiB/s | 769 MiB/s | 638 MiB/s | 464 MiB/s |
| sync_churn | **2.45 GiB/s** | 2.43 GiB/s | 713 MiB/s | 448 MiB/s | 919 MiB/s | 489 MiB/s | 154 MiB/s |
| truecolor_unique | **1.88 GiB/s** | 1.59 GiB/s | 660 MiB/s | 425 MiB/s | 942 MiB/s | 589 MiB/s | 518 MiB/s |
| sgr_nvim | **1.86 GiB/s** | 1.85 GiB/s | 636 MiB/s | 423 MiB/s | 807 MiB/s | 601 MiB/s | 294 MiB/s |
| bracketed_paste | **3.43 GiB/s** | 2.93 GiB/s | 1.37 GiB/s | 646 MiB/s | 1.35 GiB/s | 747 MiB/s | 216 MiB/s |
| osc_heavy | 3.02 GiB/s | **3.19 GiB/s** | 790 MiB/s | 256 MiB/s | 2.26 GiB/s | 2.15 GiB/s | 233 MiB/s |
| dcs_sixel | 7.95 GiB/s | **11.32 GiB/s** | 1.02 GiB/s | 978 MiB/s | 7.27 GiB/s | 4.60 GiB/s | 795 MiB/s |
| tui_redraw | **2.88 GiB/s** | 2.76 GiB/s | 770 MiB/s | 470 MiB/s | 945 MiB/s | 705 MiB/s | 177 MiB/s |
| cli_color | **3.22 GiB/s** | 3.20 GiB/s | 1020 MiB/s | 548 MiB/s | 1008 MiB/s | 503 MiB/s | 178 MiB/s |

*vt100 is a full-screen emulator, not a standalone parser.

SIMD pays off on long runs (`plain` +88%, `utf8_mixed` +55%, `dcs_sixel` +42%) and
loses on short ones (`bracketed_paste` -15%, `truecolor_unique` -15%), where the
vector setup is never amortised. That is why `Parser::with_simd` is opt-in.

## Results: `vt_screen` (parse + cell grid)

| case | vtty | vtty-simd |
| --- | --- | --- |
| plain | 225 MiB/s | **228 MiB/s** |
| sgr_nvim | 612 MiB/s | **613 MiB/s** |
| sgr_churn | 863 MiB/s | **863 MiB/s** |
| truecolor_unique | **725 MiB/s** | 684 MiB/s |
| ansi_mixed | 561 MiB/s | **567 MiB/s** |

Measured on Apple Silicon (aarch64). Numbers are medians of 20 Criterion
samples from a full-suite run; throughput is sensitive to thermal throttling
and CPU clock, so results vary run-to-run; treat as relative rankings rather
than absolute figures.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE) or
  <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](../../LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
