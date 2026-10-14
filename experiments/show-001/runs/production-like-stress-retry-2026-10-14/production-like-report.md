# Production-Like Concurrent Show-001 Test

- **Run:** `production-like-stress-retry-2026-10-14`
- **Episodes:** 10 concurrent
- **Model:** `qwen2.5:0.5b` through live Ollama
- **Panel:** 12 agents, 1 round per episode
- **Network source:** `https://www.epa.gov/`, retrieved per episode over HTTPS
- **Pipeline:** retrieval → live Ollama panel → producer packaging → gate → audit/review routing

## Executive result

The bounded retry completed all ten episodes. Live inference, network retrieval, event-history verification, producer packaging, and gate evaluation all ran to completion with no Ollama process failures, retrieval failures, or malformed gate reports.

## End-to-end stage metrics

| Stage | Mean | p50 | p95 | Min | Max |
|---|---:|---:|---:|---:|---:|
| Network retrieval | 5.41s | 5.41s | 5.42s | 5.40s | 5.42s |
| Live Ollama episode | 285.21s | 285.62s | 301.11s | 264.06s | 301.56s |
| Gate evaluation | 2.11ms | 1.88ms | 3.22ms | 1.53ms | 3.68ms |
| End-to-end episode | 290.62s | 291.03s | 306.52s | 269.46s | 306.98s |

- Batch wall time: **306.98s**
- Batch throughput: **0.033 completed episodes/sec**
- Live agent calls: **120**
- Events per episode: **13** (one question event plus 12 message events in the persisted history)

## Reliability and failure rates

| Check | Success | Failure | Rate |
|---|---:|---:|---:|
| HTTPS retrieval HTTP 200 | 10 | 0 | 0% failure |
| Ollama process completed | 10 | 0 | 0% failure |
| Signed history verified | 10 | 0 | 0% failure |
| Gate pass | 8 | 2 | 20% blocked |

### Gate outcomes

- Pass: **8**
- Review: **0**
- Reject: **2**
- Audit records: **10/10**
- Review queue records: **2**

Episodes 02 and 07 intentionally sent raw panel transcripts to the gate as regression cases. Both were rejected and queued. The other eight used a sourced producer draft and passed. Therefore the 20% gate-block rate is **fixture-driven**, not an estimate of natural production failure prevalence.

## Load finding

Ollama reported a single local generation slot (`-np 1`). Ten concurrent episodes therefore queued behind serialized model inference. This is the dominant latency component: roughly 265–302 seconds per episode versus roughly 5.4 seconds for retrieval and 1.5–3.7 milliseconds for gate evaluation.

A first run with a 240-second per-episode timeout failed at the harness boundary while Ollama was still working. The retry increased the bound to 900 seconds and completed all ten episodes. This is a meaningful operational result: **the current local Ollama configuration cannot support a 240-second SLO for ten-way concurrent 12-agent episodes.**

## Scope and next capacity step

This is production-like orchestration, not a production capacity claim. It exercises real local inference, real HTTPS retrieval, signed event persistence, producer packaging, and the gate. It does not test multiple Ollama replicas, GPU inference, a shared database, source-rate limits, or a distributed queue. The next capacity test should add parallel Ollama workers or replicas and repeat at concurrency 10, then test concurrency 25 and 50.
