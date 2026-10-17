# 25-Episode Parallel Ollama Worker Benchmark

- **Run:** `parallel-workers-25-controlled-2026-10-16`
- **Workload:** 25 episodes, 12 agents, 1 round each = **300 live agent calls**
- **Inference topology:** 4 Ollama workers × 2 slots = **8 admitted inference slots**
- **Admission control:** 25-episode workload submitted through an 8-slot controller; no more than two episodes entered any worker simultaneously
- **Model:** `qwen2.5:0.5b`
- **Network source:** `https://www.epa.gov/` retrieved per episode over HTTPS

## Executive result

All 25 episodes completed successfully through live Ollama inference, HTTPS retrieval, signed event persistence, producer packaging, and gate evaluation. The scaled configuration was reliable, but on this CPU-only sandbox it **did not improve throughput**: eight concurrent model slots created substantial CPU contention.

## Batch throughput

| Metric | Result |
|---|---:|
| Batch wall time | 2594.39s (43.24 min) |
| Completed episodes | 25/25 |
| Throughput | 0.0096 episodes/sec (0.578/min) |
| Agent calls | 300 |
| Events per episode | 13 |
| Max inference concurrency | 8 |

## Stage latency

| Stage | Mean | p50 | p95 | Min | Max |
|---|---:|---:|---:|---:|---:|
| HTTPS retrieval | 8.57s | 5.27s | 17.62s | 1.90s | 17.83s |
| Ollama episode | 809.61s | 865.98s | 1129.63s | 121.76s | 1156.92s |
| Gate evaluation | 9.25ms | 7.95ms | 11.63ms | 1.49ms | 50.31ms |
| End-to-end episode | 818.19s | 875.07s | 1135.96s | 124.61s | 1158.98s |

## Worker distribution

| Worker | Episodes | Mean Ollama | p95 Ollama | Mean end-to-end |
|---|---:|---:|---:|---:|
| worker-01 | 7 | 827.91s | 1141.93s | 837.76s |
| worker-02 | 6 | 795.94s | 1100.02s | 804.53s |
| worker-03 | 6 | 765.75s | 1052.62s | 773.70s |
| worker-04 | 6 | 845.80s | 1036.72s | 853.53s |


## Reliability and gate outcomes

| Check | Success | Failure | Failure rate |
|---|---:|---:|---:|
| HTTPS retrieval HTTP 200 | 25 | 0 | 0% |
| Ollama process completion | 25 | 0 | 0% |
| Signed history verification | 25 | 0 | 0% |
| Gate pass | 22 | 3 | 12% blocked |

Gate outcomes: **22 pass**, **3 reject**, **0 review**. Episodes 05, 15, and 25 were intentional raw-transcript regression fixtures; their rejects account for the 12% blocked rate.

## Scaling interpretation

The previous 10-episode / single-worker run completed in 306.98 seconds, or approximately 0.0326 episodes/sec. This 25-episode / four-worker, eight-slot run completed at approximately 0.0096 episodes/sec. The parallel CPU configuration therefore delivered lower observed throughput, not because of retrieval or gate overhead, but because multiple local model servers contended for the same CPU resources and each episode's 12 sequential calls became much slower.

This is a valid capacity finding: **horizontal Ollama workers alone are not beneficial on this CPU-only host when each worker is allowed two active generations.** For improved throughput, the next test should use GPU-backed inference, fewer active slots per CPU host, or separate hosts/VMs with dedicated CPU capacity. The 25-episode workload and admission-control design are reusable for that comparison.

## Harness lessons

An earlier all-at-once 25-thread attempt oversubscribed four one-slot workers and failed in the harness's timeout-output handling before producing metrics. The controlled rerun fixed byte-safe timeout capture and bounded admission at eight active inference slots. It completed all 25 episodes and produced a complete measurement.
