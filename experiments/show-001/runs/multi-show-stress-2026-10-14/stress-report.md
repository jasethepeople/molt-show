# Multi-Show Gate Stress Test

- **Run:** `multi-show-stress-2026-10-14`
- **Episodes:** 10
- **Concurrency:** 10
- **Gate binary:** optimized Rust release build (`target/release/gate`)
- **Scope:** deterministic pre-publication gate path only; no concurrent Ollama generation or network source retrieval

## Executive result

All ten episode fixtures completed and matched their expected verdicts. The gate remained responsive under ten concurrent processes, with no crashes, timeouts, malformed reports, or queue-write failures.

## Latency and throughput

| Metric | Result |
|---|---:|
| Wall-clock batch time | 15.43 ms |
| Sum of per-process latency | 49.71 ms |
| Mean latency | 4.97 ms |
| Median / p50 | 4.82 ms |
| p95 | 6.56 ms |
| Min / max | 3.20 / 6.66 ms |
| Observed batch throughput | 648.2 episodes/sec |

Because process startup is included in each measurement and this is a short local run, these numbers are a baseline for this sandbox rather than a production capacity commitment.

## Verdict and failure rates

| Outcome | Episodes | Rate |
|---|---:|---:|
| Pass | 4 | 40% |
| Review | 3 | 30% |
| Reject | 3 | 30% |
| Non-pass / publication blocked | 6 | 60% |
| Fixture verdict mismatches | 0 | 0% |

The **60% non-pass rate is fixture-driven**, not an observed system defect: the workload intentionally contained three `review` cases and three `reject` cases to exercise the blocking paths.

## Durability checks

- Audit records written: **10/10**
- Review queue records written: **6**
- Each non-pass episode produced exactly one queue item.
- Each episode produced exactly one audit record.
- Each process returned exit code `0` only for `pass`; `review` and `reject` returned exit code `1`.
- Expected/verdict mismatches: **0**

## Episode matrix

| Episode | Expected | Actual | Latency (ms) | Findings | Queue |
|---|---|---|---:|---:|---:|
| episode-01 | pass | pass | 3.20 | 0 | 0 |
| episode-02 | review | review | 6.66 | 1 | 1 |
| episode-03 | reject | reject | 4.21 | 1 | 1 |
| episode-04 | pass | pass | 4.68 | 0 | 0 |
| episode-05 | review | review | 4.97 | 1 | 1 |
| episode-06 | reject | reject | 6.45 | 1 | 1 |
| episode-07 | pass | pass | 6.13 | 0 | 0 |
| episode-08 | review | review | 4.01 | 1 | 1 |
| episode-09 | reject | reject | 5.16 | 1 | 1 |
| episode-10 | pass | pass | 4.24 | 0 | 0 |

## Interpretation

The gate is CPU-local and deterministic, so the ten-process batch completed in approximately the same order of magnitude as a single invocation. The test demonstrates concurrency behavior and failure routing for the publication boundary. It does **not** measure model inference throughput, source-fetch latency, queue contention against a shared database, or production filesystem performance. Those require a separate benchmark with the live show runner, research workers, and shared persistence backend.
