# Capacity-Aware Admission Control
## Automated Rollout Plan and Monitoring Dashboard Specification

**Status:** Proposed  
**Scope:** Episode intake, inference-worker admission, autoscaling decisions, rollout safety, observability, and rollback  
**Baseline workload:** 25 episodes, 12 agents, 1 round, 300 live agent calls  
**Related strategy:** `optimized-execution-strategy.md`

---

## 1. Objectives and non-goals

### Objectives

- Prevent episode bursts from oversubscribing Ollama or GPU capacity.
- Admit work according to measured worker slots, not requested episode count.
- Preserve signed event ordering and prevent duplicate generation after retries.
- Roll out new worker-pool configurations gradually and automatically.
- Detect CPU contention, GPU saturation, queue growth, tail-latency regressions, and worker failures.
- Make every promotion and rollback decision auditable.

### Non-goals

- Parallelizing causally ordered turns inside one episode.
- Automatically publishing content that has not passed the gate and review policy.
- Spawning unlimited Ollama processes when the queue grows.
- Treating queue depth alone as an infrastructure failure.

---

## 2. Target control plane

```text
                 +----------------------+
                 | Episode intake API   |
                 +----------+-----------+
                            |
                     durable episode queue
                            |
                 +----------v-----------+
                 | Admission controller |
                 | global + worker caps  |
                 +----+-------------+----+
                      |             |
              +-------v----+  +---v--------+
              | CPU pool   |  | GPU pool    |
              | warm Ollama|  | warm Ollama |
              +-------+----+  +---+---------+
                      |             |
                      +------+------+ 
                             |
                     signed event writer
                             |
                 +-----------v-----------+
                 | Gate and review queue |
                 +-----------+-----------+
                             |
                     metrics/event stream
                             |
                 +-----------v-----------+
                 | Dashboard + alerting  |
                 +------------------------+
```

The admission controller is the authoritative capacity boundary. All episode launches must pass through it. Direct episode-to-Ollama calls are prohibited in production mode.

---

## 3. Admission-control contract

### Worker registration

Each worker registers:

```json
{
  "worker_id": "gpu-worker-01",
  "backend": "ollama",
  "model": "qwen2.5:0.5b",
  "model_digest": "sha256:...",
  "capacity_slots": 2,
  "available_slots": 2,
  "health": "ready",
  "hardware": {"kind": "gpu", "device": "0"}
}
```

A worker is eligible for admission only when:

- its health check is passing;
- its model and digest match the deployment configuration;
- it has at least one available slot;
- its queue age is below the worker-specific limit;
- it has not tripped the circuit breaker.

### Admission decision

```text
if global_in_flight >= global_limit:
    queue episode
else if no healthy worker has an available slot:
    queue episode
else:
    select least-loaded eligible worker
    reserve one slot atomically
    start episode
```

The slot reservation must be atomic. A worker must never observe two successful reservations for the same slot.

### Queue states

```text
accepted -> queued -> admitted -> running -> completed
                              |          |
                              |          +-> failed -> retryable | terminal
                              +-> expired
```

Every state transition emits an audit event with `episode_id`, `worker_id`, timestamp, and reason.

### Idempotency

Every submission carries an idempotency key:

```text
show_id + episode_id + execution_attempt
```

A retry may create a new execution attempt, but it may not append duplicate signed events to the original attempt. If an episode has already committed an event sequence, recovery must resume from the last verified sequence or create an explicit replacement attempt.

---

## 4. Automated rollout state machine

### States

```text
CANDIDATE
   |
   v
SHADOW -> CANARY_5 -> CANARY_25 -> CANARY_50 -> PROMOTED
   |          |           |            |
   +----------+-----------+------------+
                ROLLBACK
```

### State definitions

| State | Traffic share | Purpose |
|---|---:|---|
| Candidate | 0% | Validate configuration and worker health |
| Shadow | 0% live / mirrored metrics | Compare scheduling decisions without serving production work |
| Canary 5 | 5% | Detect obvious failures with limited exposure |
| Canary 25 | 25% | Measure queueing and latency under meaningful load |
| Canary 50 | 50% | Validate scaling behavior and fairness |
| Promoted | 100% | Active configuration becomes default |
| Rollback | 0% candidate | Restore last-known-good configuration |

### Candidate validation

Before traffic is enabled, automation must verify:

- configuration schema and bounds;
- model availability and digest;
- worker health for every intended target;
- capacity slots are positive and within policy limits;
- queue and event-store connectivity;
- gate binary availability;
- no stale circuit breaker or orphaned reservation state;
- a one-episode synthetic smoke test with signed-history verification.

### Promotion gates

A stage may advance only when its observation window is complete and all hard gates pass.

**Minimum observation windows:**

- Candidate smoke test: 1 complete synthetic episode
- Canary 5: at least 5 episodes or 10 minutes
- Canary 25: at least 25 episodes or 30 minutes
- Canary 50: at least 50 episodes or 60 minutes
- Promotion: two consecutive healthy windows at the current stage

**Hard gates:**

- Episode infrastructure completion ≥ 99%
- Signed-history verification = 100% for completed episodes
- Worker process error rate < 1%
- Inference timeout rate < 1%
- No event-ordering or duplicate-event violations
- Gate remains available and blocks intentional regression fixtures
- Queue age p95 does not exceed the configured limit
- No active circuit breaker on a required worker class

**Soft gates:**

- Throughput improves or remains within 10% of the best known configuration
- p95 episode latency does not regress by more than 25%
- GPU utilization is productive but not persistently saturated at 100%
- CPU load and runnable-process count remain within host policy

Any hard-gate failure immediately pauses promotion. Two soft-gate failures in one window trigger rollback review.

---

## 5. Automated rollout procedure

### Step 1 — Register candidate

Create a versioned rollout record:

```json
{
  "rollout_id": "rollout-2026-10-17-001",
  "candidate": {
    "workers": 1,
    "slots_per_worker": 1,
    "backend": "ollama",
    "model": "qwen2.5:0.5b",
    "model_digest": "sha256:..."
  },
  "previous_good": "config-2026-10-16-003",
  "stage": "CANDIDATE"
}
```

### Step 2 — Run preflight

The rollout controller performs configuration validation, worker health checks, model-digest checks, and one synthetic episode. It records the results but does not mutate the active configuration if preflight fails.

### Step 3 — Enter shadow mode

In shadow mode, the candidate receives scheduling decisions and synthetic or mirrored metadata but does not own live episode execution. Compare:

- predicted worker selection;
- expected queue wait;
- estimated capacity;
- fairness across workers;
- admission/rejection decisions.

### Step 4 — Enable 5% canary

Route 5% of new episodes to the candidate. Existing episodes remain on their original worker. Do not migrate an in-flight episode between worker pools.

### Step 5 — Evaluate and advance

At the end of the canary window, compute the promotion gates. If healthy, advance to 25%, then 50%, then 100%. If unhealthy, freeze the rollout and execute rollback.

### Step 6 — Promote and retain rollback

When promoted:

- mark candidate as active;
- retain the previous configuration for immediate rollback;
- preserve all rollout observations;
- start a post-promotion watch window;
- do not garbage-collect the previous worker pool until the watch window ends.

### Step 7 — Close rollout

After two healthy post-promotion windows, mark the rollout complete. Persist a summary with throughput, latency, error rate, gate outcomes, and rollback readiness.

---

## 6. Rollback design

### Automatic rollback triggers

Rollback immediately when any of the following occurs:

- signed-history verification failure;
- duplicate or out-of-order event detected;
- inference timeout rate ≥ 2% in a 10-minute window;
- worker process error rate ≥ 2% in a 10-minute window;
- queue age p95 exceeds the hard limit for two consecutive windows;
- all workers in a required pool are unhealthy;
- gate unavailable or gate behavior deviates from the known regression fixtures;
- memory pressure, GPU OOM, or repeated model-load failure;
- candidate throughput falls more than 25% below the previous good configuration.

### Rollback actions

1. Freeze promotion and stop new candidate admissions.
2. Leave already-running episodes to complete unless they are unsafe or stuck past timeout.
3. Route new episodes to the previous known-good pool.
4. Cancel unstarted candidate reservations and release slots.
5. Mark candidate attempts as `rolled_back`, not `failed`.
6. Reconcile queue reservations and identify orphaned jobs.
7. Verify event histories for all episodes that crossed the candidate boundary.
8. Emit a rollback report and page the on-call owner.

Rollback must not delete episode data, audit records, gate reports, or signed histories.

---

## 7. Monitoring dashboard specification

### Dashboard A — Executive capacity overview

Purpose: answer “Can the system accept more episodes safely?”

Panels:

1. **Active rollout state**
   - Current stage: candidate, canary, promoted, rollback
   - Rollout ID and candidate configuration
   - Time in current stage

2. **Episodes per minute**
   - Completed episodes/minute
   - Compare candidate vs previous-good baseline

3. **Queue health**
   - Queue depth
   - Queue age p50, p95, and max
   - Expired jobs

4. **Inference capacity**
   - Total slots
   - Used slots
   - Available slots
   - Admission utilization percentage

5. **End-to-end latency**
   - p50, p95, and p99 episode latency
   - Queue wait separated from generation time

6. **Reliability scorecard**
   - Episode completion rate
   - Worker error rate
   - Timeout rate
   - Signed-history verification rate
   - Gate availability

7. **Current decision**
   - `PROMOTE`, `HOLD`, or `ROLLBACK`
   - Top reason and threshold breached

### Dashboard B — Worker and hardware detail

Purpose: identify contention and uneven capacity.

Panels per worker:

- worker health state;
- worker ID and model digest;
- active slots / configured slots;
- queue depth and queue age;
- generation requests per minute;
- generation latency p50/p95;
- tokens/sec, if available;
- retry and timeout count;
- CPU utilization and load average;
- memory usage and model residency;
- GPU utilization, VRAM used/free, temperature, and power when applicable;
- Ollama model-load count and load duration.

Use a heatmap for worker latency and a stacked time series for `queued`, `running`, `completed`, and `failed` jobs.

### Dashboard C — Pipeline stage breakdown

Purpose: ensure inference remains the actual bottleneck and detect regressions elsewhere.

Panels:

- research retrieval latency and status code distribution;
- research cache hit rate;
- queue wait duration;
- per-agent generation duration;
- event-write latency;
- history-verification duration and failures;
- gate evaluation latency and verdict distribution;
- review-queue backlog;
- end-to-end critical-path waterfall.

### Dashboard D — Rollout and audit

Purpose: make automated promotion explainable.

Panels:

- rollout timeline by stage;
- candidate vs previous-good throughput;
- candidate vs previous-good p50/p95 latency;
- hard-gate and soft-gate status;
- promotion decisions and reasons;
- rollback count and cause;
- configuration and model-digest changes;
- synthetic smoke-test results;
- intentional gate-regression fixture results.

---

## 8. Metrics catalogue

### Admission metrics

```text
molt_admission_queue_depth{pool,model}
molt_admission_queue_age_seconds{pool,quantile}
molt_admission_reservations_total{pool,result}
molt_admission_slots{pool,state="available|reserved|running"}
molt_admission_expired_total{pool,reason}
molt_admission_rejections_total{pool,reason}
```

### Inference metrics

```text
molt_inference_requests_total{worker,model,status}
molt_inference_queue_wait_seconds{worker,quantile}
molt_inference_generation_seconds{worker,quantile}
molt_inference_tokens_per_second{worker}
molt_inference_timeouts_total{worker,reason}
molt_inference_retries_total{worker,reason}
molt_inference_model_load_seconds{worker,model}
```

### Episode and integrity metrics

```text
molt_episode_duration_seconds{pool,quantile}
molt_episode_completed_total{pool,result}
molt_episode_event_count{pool}
molt_history_verification_total{result}
molt_duplicate_event_total{episode}
molt_event_order_violation_total{episode}
```

### Gate and publication metrics

```text
molt_gate_evaluations_total{verdict}
molt_gate_duration_seconds{quantile}
molt_review_queue_depth{reason}
molt_publish_blocked_total{reason}
```

### Hardware metrics

```text
molt_host_cpu_utilization_ratio{host}
molt_host_load_average{host}
molt_host_memory_utilization_ratio{host}
molt_gpu_utilization_ratio{worker,gpu}
molt_gpu_memory_used_bytes{worker,gpu}
molt_gpu_memory_available_bytes{worker,gpu}
molt_gpu_temperature_celsius{worker,gpu}
```

All metrics must include a stable `rollout_id` or a low-cardinality `config_version` where comparison between candidate and previous-good is required. Do not attach raw episode text, prompts, source contents, or user identifiers to metric labels.

---

## 9. Alert rules

### Page immediately

- `molt_history_verification_total{result="failure"} > 0`
- `molt_duplicate_event_total > 0`
- `molt_event_order_violation_total > 0`
- all eligible workers unhealthy for more than 60 seconds;
- GPU OOM or repeated model-load failure;
- gate unavailable for more than 30 seconds;
- active rollout hard-gate failure.

### Page after sustained breach

- episode completion error rate ≥ 2% for 10 minutes;
- inference timeout rate ≥ 2% for 10 minutes;
- queue age p95 above 10 minutes for 10 minutes;
- queue depth increasing for 15 consecutive minutes;
- candidate throughput ≥ 25% below previous-good for 10 minutes.

### Ticket or warning

- CPU utilization above 90% for 15 minutes while tokens/sec declines;
- GPU utilization below 30% while queue age grows;
- GPU memory above 90% for 10 minutes;
- worker latency coefficient of variation above 0.5;
- research retrieval error rate above 1%;
- gate review queue grows above its operating threshold;
- retry rate above 1%.

Alerts should include the rollout ID, pool, worker IDs, current queue depth, active slots, p95 queue age, p95 generation latency, and recommended action.

---

## 10. Capacity controller configuration

Example policy for the current CPU host:

```yaml
pool: cpu-default
mode: bounded
workers: 1
slots_per_worker: 1
global_in_flight: 1
queue:
  max_depth: 100
  max_age_seconds: 3600
  expiry_action: mark_expired_and_alert
admission:
  routing: least_loaded
  reserve_retry_slots: 1
  max_episode_retries: 1
  retryable_errors:
    - transport_error
    - worker_restart
    - model_load_failure
  non_retryable_errors:
    - signed_event_conflict
    - gate_policy_failure
  episode_timeout_seconds: 1200
rollout:
  canary_steps: [5, 25, 50, 100]
  min_healthy_windows: 2
  auto_rollback: true
```

Example policy for a GPU pilot:

```yaml
pool: gpu-default
mode: bounded
workers: 1
slots_per_worker: 1
max_in_flight: 1
scale_policy:
  allowed_slots: [1, 2, 4]
  promote_only_if_throughput_gain_percent: 10
  rollback_if_p95_regression_percent: 25
  rollback_if_gpu_memory_percent: 95
hardware_policy:
  require_gpu: true
  min_free_vram_percent: 15
```

These values are starting policies. The rollout controller must replace them with measured limits after the CPU matrix and GPU pilot.

---

## 11. Runbook actions

### Queue growing, workers healthy

1. Check queue age versus SLO.
2. Check generation latency and tokens/sec.
3. Do not increase slots automatically on CPU.
4. If GPU utilization is low and VRAM is available, test the next approved slot level.
5. If all capacity is saturated, retain the queue and communicate estimated start time.

### CPU saturated, throughput falling

1. Hold rollout.
2. Reduce active slots to the last known-good setting.
3. Keep one warm worker and compare against the baseline.
4. Do not add more local workers without dedicated CPU capacity.

### GPU memory pressure

1. Stop slot increases.
2. Drain new admissions to the previous configuration.
3. Allow running episodes to finish if within timeout.
4. Reduce slots or context length only through a versioned rollout.
5. Roll back if OOM repeats.

### Worker failure

1. Mark the worker unhealthy.
2. Stop new reservations for that worker.
3. Requeue only unstarted or explicitly retryable jobs.
4. Verify no partial signed history is being silently continued.
5. Re-admit the worker only after health check, model-digest check, and smoke test pass.

### Gate or integrity failure

1. Stop publication immediately.
2. Keep inference available only for quarantined drafts if policy allows.
3. Preserve all event histories and reports.
4. Page the owner and roll back the candidate configuration.

---

## 12. Delivery sequence

1. Add queue, reservation, and worker-state metrics.
2. Implement atomic slot reservation and idempotency keys.
3. Add rollout state persistence and candidate/previous-good comparison.
4. Add synthetic smoke tests and gate regression fixtures to preflight.
5. Build the four dashboard views.
6. Configure hard and warning alerts.
7. Run the CPU tuning matrix: C1 through C6.
8. Select the best CPU configuration.
9. Run the GPU slot pilot if GPU capacity becomes available.
10. Enable automated canary promotion with rollback.
11. Run a 25-episode production-like canary with human review enabled.
12. Publish a rollout report with throughput, p95 latency, queue age, integrity, and gate outcomes.

---

## 13. Acceptance criteria

The admission-control system is ready for automated rollout when:

- all episode submissions are idempotent;
- no direct unbounded inference path remains;
- every reservation has a matching release or explicit reconciliation record;
- queue wait and generation time are separately observable;
- signed histories verify for 100% of completed canary episodes;
- gate regression fixtures produce expected verdicts;
- rollout promotion and rollback are tested in a non-production environment;
- dashboard panels show candidate versus previous-good performance;
- alerts include actionable context;
- a rollback returns new admissions to the previous configuration without deleting data;
- a 25-episode canary completes within the selected throughput and latency SLOs.

**Primary success metric:** completed episodes per minute at acceptable p95 latency, with zero integrity violations—not the number of Ollama workers or configured slots.
