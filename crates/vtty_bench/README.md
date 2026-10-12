# vtty_bench

Criterion throughput benchmarks comparing `vtty` (scalar and SIMD) against
`vte`, `vt100`, `vtparse`, and `rio-vt`.

```bash
cargo bench -p vtty_bench
```

## Method

2 MiB byte corpora, counting sinks (so the numbers reflect parser + dispatch
cost, not grid mutation), median throughput.

Corpora: `plain`, `scroll_storm`, `utf8_mixed`, `ansi_mixed`,
`ansi_two_styles`, `ansi_bare_csi`, `sgr_churn`, `cursor_motion`, `sync_churn`,
`truecolor_unique`, `bracketed_paste`, `osc_heavy`, `dcs_sixel`.

## Results

| case | vtty | vtty-simd | vte | vtparse | rio-vt | vt100* |
| --- | --- | --- | --- | --- | --- | --- |
| plain | 2.54 GiB/s | **12.72 GiB/s** | 1.37 GiB/s | 403 MiB/s | 1.46 GiB/s | 133 MiB/s |
| scroll_storm | 1.55 GiB/s | **1.65 GiB/s** | 1.34 GiB/s | 375 MiB/s | 1.31 GiB/s | 74 MiB/s |
| utf8_mixed | 818 MiB/s | 1.21 GiB/s | 823 MiB/s | 225 MiB/s | 1.21 GiB/s | 171 MiB/s |
| ansi_mixed | **682 MiB/s** | 680 MiB/s | 314 MiB/s | 188 MiB/s | 386 MiB/s | 176 MiB/s |
| ansi_two_styles | 847 MiB/s | **856 MiB/s** | 332 MiB/s | 194 MiB/s | 354 MiB/s | 160 MiB/s |
| ansi_bare_csi | 1.14 GiB/s | **1.24 GiB/s** | 492 MiB/s | 210 MiB/s | 459 MiB/s | 155 MiB/s |
| sgr_churn | **675 MiB/s** | 648 MiB/s | 335 MiB/s | 181 MiB/s | 439 MiB/s | 229 MiB/s |
| cursor_motion | **681 MiB/s** | 647 MiB/s | 216 MiB/s | 192 MiB/s | 420 MiB/s | 271 MiB/s |
| sync_churn | 726 MiB/s | **738 MiB/s** | 294 MiB/s | 234 MiB/s | 483 MiB/s | 103 MiB/s |
| truecolor_unique | **680 MiB/s** | 634 MiB/s | 340 MiB/s | 207 MiB/s | 478 MiB/s | 262 MiB/s |
| bracketed_paste | **1.12 GiB/s** | 1.04 GiB/s | 733 MiB/s | 283 MiB/s | 739 MiB/s | 150 MiB/s |
| osc_heavy | 1.67 GiB/s | **1.71 GiB/s** | 366 MiB/s | 133 MiB/s | 1.19 GiB/s | 153 MiB/s |
| dcs_sixel | 4.38 GiB/s | **4.52 GiB/s** | 581 MiB/s | 437 MiB/s | 4.15 GiB/s | 461 MiB/s |

*vt100 is a full-screen emulator, not a standalone parser.

`utf8_mixed` is a statistical tie between `vtty-simd` and `rio-vt`
(≈ 1.21 GiB/s); the aligned 64-byte-block UTF-8→UTF-32 transcode closed what
was previously an ~8% gap. Numbers carry ~10% run-to-run variance (AVX
frequency scaling).
