# Optimized Execution Strategy
## GPU Acceleration and Worker Pooling for MOLT Show-001

**Status:** Proposed  
**Scope:** Live episode inference, source retrieval, gate evaluation, and workload admission  
**Baseline:** 25 episodes, 12 agents, 1 round, 300 live agent calls  
**Benchmark reference:** `parallel-workers-25-controlled-2026-10-16`

---

## 1. Executive recommendation

Use a **single logical execution service with explicit capacity-aware worker pooling**, rather than launching one unconstrained Ollama process per episode.

There are two execution paths:

1. **Immediate CPU path:** Keep one warm Ollama model worker as the default, begin with one active episode per worker, and tune concurrency empirically. Separate the lightweight retrieval, persistence, and gate stages from model-generation capacity.
2. **Preferred production path:** Move inference to a GPU-backed worker pool. Keep the model resident on GPU, use a bounded number of generation slots based on VRAM and measured tokens-per-second, and scale horizontally only by adding genuinely dedicated GPU capacity.

The central design rule is:

> Scale admission to available inference capacity, not to the number of requested episodes.

The current sandbox has **6 vCPUs, 23 GiB RAM, and no visible GPU**. The benchmark showed that four local Ollama workers with eight active generations reduced throughput because they competed for the same CPU resources.

---

## 2. What the benchmark tells us

The controlled benchmark completed all 25 episodes reliably, but model inference dominated the pipeline:

| Metric | Observed result |
|---|---:|
| Episodes | 25 |
| Agents per episode | 12 |
| Live agent calls | 300 |
| Workers | 4 Ollama workers |
| Active generation slots | 8 |
| Batch wall time | 43.24 minutes |
| Throughput | 0.0096 episodes/sec |
| Mean Ollama episode time | 809.61 sec |
| Mean gate time | 9.25 ms |
| HTTPS retrieval success | 25/25 |
| Signed history verification | 25/25 |

The earlier single-worker run achieved approximately **0.0326 episodes/sec**, compared with **0.0096 episodes/sec** in the multi-worker CPU run. This is not evidence that the MOLT runtime or gate is slow. It is evidence that CPU-bound local inference was oversubscribed.

### Bottleneck ranking

1. **Model generation:** dominant cost and source of contention.
2. **CPU scheduling and context competition:** worsens as active local generations increase.
3. **Network retrieval:** measurable but not dominant; average approximately 8.57 seconds.
4. **Gate evaluation:** negligible relative to inference; average approximately 9.25 milliseconds.
5. **Event persistence and signature verification:** operationally successful and not a measured bottleneck.

---

## 3. Target architecture

```text
                    +----------------------+
                    | Episode intake queue |
                    +----------+-----------+
                               |
                    capacity-aware admission
                               |
            +------------------+------------------+
            |                                     |
   +--------v---------+                 +---------v--------+
   | Research pool    |                 | Inference pool   |
   | async HTTP/cache |                 | GPU or CPU Ollama|
   +--------+---------+                 +---------+--------+
            |                                     |
            +------------------+------------------+
                               |
                    +----------v-----------+
                    | Event writer         |
                    | signed JSONL history |
                    +----------+-----------+
                               |
                    +----------v-----------+
                    | Pre-publication gate|
                    +----------+-----------+
                               |
                    +----------v-----------+
                    | Review / publish    |
                    +----------------------+
```

### Separation of concerns

- **Episode queue:** accepts work and assigns an episode ID.
- **Research pool:** performs HTTPS retrieval, caching, and source normalization without consuming model-generation slots.
- **Inference pool:** owns Ollama clients and limits concurrent live generations.
- **Event writer:** serializes signed events and preserves per-episode ordering.
- **Gate pool:** evaluates producer drafts after inference; it can run with high concurrency because it is inexpensive.
- **Review/publish stage:** remains downstream of the gate and human-review policy.

The inference pool should be the only component allowed to determine whether another live episode may begin model generation.

---

## 4. Immediate CPU strategy for the current host

### Recommended starting configuration

| Setting | Initial value |
|---|---:|
| Ollama model workers | 1 warm worker |
| Active generation slots | 1 |
| Episode admission | 1 episode at a time per worker |
| Research concurrency | 8–16 async requests, with per-domain limits |
| Gate concurrency | 8 or higher |
| Model residency | Keep model loaded between episodes |
| Episode timeout | 20 minutes initially |
| Retry policy | One retry for transport/process failure only |
| Generation retry | Never silently regenerate a committed event |

The goal is to establish a clean CPU baseline before increasing concurrency. A single worker with one active slot may outperform many workers because it avoids model-server and scheduler contention. This must be measured with the same 25-episode workload rather than assumed.

### CPU tuning matrix

Run the identical workload under these configurations:

| Test | Workers | Slots/worker | Total slots | Purpose |
|---|---:|---:|---:|---|
| C1 | 1 | 1 | 1 | Clean CPU baseline |
| C2 | 1 | 2 | 2 | Test model-server parallelism |
| C3 | 2 | 1 | 2 | Test horizontal CPU scaling |
| C4 | 2 | 2 | 4 | Test moderate contention |
| C5 | 4 | 1 | 4 | Reproduce distributed one-slot case |
| C6 | 4 | 2 | 8 | Existing stressed configuration |

Stop increasing concurrency when any of the following occurs:

- Throughput decreases by more than 10% from the best configuration.
- p95 episode latency increases by more than 25%.
- CPU remains saturated while tokens/sec per generation falls.
- Timeout, queue wait, or Ollama process errors increase.

### CPU-specific optimization

- Use one persistent Ollama process per worker instead of starting a model server per request.
- Keep the model warm with a bounded idle timeout.
- Avoid running four independent model copies unless each has dedicated CPU capacity.
- Pin workers to CPU sets only if measurement shows improved cache locality; do not assume pinning helps.
- Keep retrieval and gate work off the inference threads.
- Use a queue and semaphore instead of launching one shell process per episode.
- Record queue wait separately from generation time; otherwise contention is hidden inside episode latency.

---

## 5. Preferred GPU strategy

### GPU topology

Start with one GPU-backed Ollama worker and scale slots only after measuring VRAM and tokens/sec.

```text
Episode queue
     |
GPU admission controller
     |
+----+-------------------+
| GPU worker 1           |
| resident model         |
| slots tuned to VRAM    |
+------------------------+
```

If one GPU is insufficient:

```text
                 +----------------+
                 | Global queue   |
                 +-------+--------+
                         |
          +--------------+--------------+
          |                             |
 +--------v--------+           +--------v--------+
 | GPU worker A    |           | GPU worker B    |
 | model resident  |           | model resident  |
 +-----------------+           +-----------------+
```

Each GPU worker should own a stable model instance and a local semaphore. The global scheduler should route only to workers with available capacity.

### GPU slot-tuning procedure

Do not choose `OLLAMA_NUM_PARALLEL` from episode count. Tune it experimentally:

1. Start with `OLLAMA_NUM_PARALLEL=1`.
2. Warm the model with one complete episode.
3. Measure tokens/sec, GPU utilization, VRAM use, p50/p95 episode latency, and queue wait.
4. Increase to 2, then 4, then 8 only while throughput improves and p95 remains within the service objective.
5. Select the highest stable setting, not the highest setting that merely starts successfully.

A slot is acceptable only when it increases completed episodes per minute without causing runaway tail latency.

### GPU admission policy

Use two limits:

- **Global in-flight episode limit:** protects the entire service.
- **Per-GPU generation-slot limit:** protects VRAM and model-server stability.

For example:

```text
max_in_flight_episodes = number of GPUs × tested stable slots per GPU
```

Episodes beyond that limit remain queued. Queue depth should be visible to operators and should not be mistaken for model failure.

### GPU placement options

**Preferred:** One model instance per GPU, with local parallel slots tuned to that GPU's VRAM.  
**Acceptable:** One GPU with multiple slots if the model fits comfortably with the desired context length.  
**Avoid:** Multiple independent model copies fighting for one GPU unless the runtime explicitly supports the placement and the benchmark proves a gain.

---

## 6. Worker-pool implementation plan

### Worker contract

Each inference worker should expose a narrow contract:

```text
submit(episode_id, model, prompt_context) -> generation_job_id
status(generation_job_id) -> queued | running | complete | failed
result(generation_job_id) -> response + usage + timing metadata
```

Required metadata per generation:

- episode ID
- agent ID
- worker ID
- model name and digest
- queue-entered timestamp
- generation-start timestamp
- generation-completed timestamp
- prompt and completion token counts, if available
- retry count
- failure category

### Scheduling policy

Use weighted least-loaded routing:

1. Filter workers with an available slot.
2. Prefer the worker with the lowest normalized queue depth.
3. Break ties by least recent assignment.
4. Reserve capacity for retries so one failing worker cannot consume the entire queue.
5. Apply circuit breaking after repeated transport or process failures.

### Backpressure

When all inference slots are occupied:

- Keep the episode in the durable queue.
- Do not spawn another Ollama process automatically.
- Return queue position and estimated start time to the operator layer.
- Apply a maximum queue age; expired jobs become explicit failures requiring requeue or review.

### Episode ordering and event integrity

The current runtime commits signed events sequentially. Preserve that invariant:

- An episode may have multiple agent calls in flight only if the world model explicitly permits it.
- Do not parallelize turns within one shared world unless event ordering and state hashes are redesigned.
- Parallelize **across episodes**, not across causally ordered turns in the same episode.
- Write event records atomically and verify the final history before gate submission.

---

## 7. Recommended phased rollout

### Phase 0 — Instrumentation

Add or expose these metrics before changing production topology:

- inference queue wait time
- generation time per agent call
- tokens/sec per worker
- CPU utilization and load average
- GPU utilization, VRAM usage, and temperature when applicable
- active slots and queue depth
- per-episode end-to-end latency
- retries, timeouts, and circuit-breaker events

**Exit criterion:** every episode can be attributed to queue time, inference time, retrieval time, gate time, and persistence time.

### Phase 1 — CPU baseline

Run C1 through C6 using the same 25-episode workload and model digest.

**Exit criterion:** select the CPU configuration with the highest completed episodes/minute subject to p95 latency and zero infrastructure failures.

### Phase 2 — GPU pilot

Use one GPU worker. Test slot counts 1, 2, 4, and 8. Keep prompts, rounds, model, and episode count constant.

**Exit criterion:** GPU configuration must beat the selected CPU baseline by at least 2× throughput without increasing gate or event-integrity failures.

### Phase 3 — Multi-GPU scale-out

Add a second GPU worker only if the first GPU is saturated and queue wait remains material. Repeat the same workload and compare near-linear scaling.

**Exit criterion:** adding a GPU worker should increase throughput by at least 60% of the ideal increment. If it does not, investigate scheduler, network, model loading, or shared-storage bottlenecks.

### Phase 4 — Production canary

Run a small live canary with human review enabled for every episode. Keep the CPU path available as a fallback.

**Exit criterion:** no unreviewed content bypasses the gate; all histories verify; p95 latency and queue age remain within the service objective for two consecutive canary windows.

---

## 8. Suggested service-level objectives

These are initial operating targets to validate during the pilot, not claims about current performance:

| SLO | Initial target |
|---|---:|
| Episode completion success | ≥ 99% excluding intentional gate rejects |
| Signed-history verification | 100% of completed episodes |
| HTTPS retrieval success | ≥ 99% with bounded retry |
| Gate availability | ≥ 99.9% |
| Gate evaluation p95 | < 100 ms |
| Inference queue wait p95 | < 10% of total episode latency |
| Episode inference timeout rate | < 1% |
| Worker process error rate | < 1% |
| GPU utilization during sustained load | Tune toward 70–90%, not 100% by default |

The most important capacity SLO is **completed episodes per minute at an acceptable p95 latency**, not raw worker count.

---

## 9. Decision rule

Choose the deployment path using measured results:

- **Stay CPU-only** if a low-slot configuration meets the required episode throughput and latency.
- **Use one GPU worker** if it provides a clear throughput improvement and stable tail latency.
- **Use multiple GPUs** only when one GPU is saturated and the second worker produces meaningful incremental throughput.
- **Use separate CPU hosts** when GPU access is unavailable but workload isolation is possible.
- **Do not use unconstrained local worker multiplication** on a shared CPU host.

### Preferred near-term decision

For the current sandbox, the recommended next run is:

```text
1 warm Ollama worker
1 active inference slot
25-episode benchmark
same model, prompt, rounds, retrieval, and gate
```

Then compare it against a GPU worker with slots 1, 2, and 4. This produces a clean capacity curve and avoids repeating the previous CPU oversubscription pattern.

---

## 10. Final operating principle

The MOLT show is a causally ordered, event-signed system. The correct optimization is not to parallelize everything. It is to parallelize the independent unit—the episode—while preserving ordered turns within each world and placing a hard capacity boundary around inference.

**Recommended architecture:**

- asynchronous research and retrieval
- durable episode queue
- bounded GPU or CPU inference pool
- sequential event commits per episode
- parallel gate evaluation
- mandatory history verification and human review before publication

This approach addresses the measured bottleneck directly, preserves the kernel invariants, and makes future GPU or multi-host comparisons reproducible.
