# MOLT Show Disaster Recovery and Failover Runbook
## Redis StatefulSet and Admission Controller Replicas

**Status:** Proposed  
**Scope:** Durable episode queue, Redis StatefulSet, admission-controller replicas, rollout state, reservations, audit records, and recovery validation  
**Primary deployment bundle:** `deploy/k8s/`  
**Audience:** On-call operators, platform engineers, release managers, and incident commanders

---

## 1. Purpose and operating principles

This runbook defines how to recover the MOLT Show capacity-control plane after Redis, admission-controller, node, storage, or cluster failure.

The recovery design follows five rules:

1. **Do not silently lose queue state.** Every accepted episode must remain queued, completed, expired, or explicitly reconciled.
2. **Do not duplicate signed events.** A retry creates a new execution attempt; it must not append duplicate events to an existing attempt.
3. **Do not publish during integrity uncertainty.** If Redis state, event history, or gate state cannot be reconciled, pause publication and route work to review.
4. **Prefer fail-closed admission.** When capacity or state is uncertain, stop new admissions rather than over-admit workers.
5. **Restore from verified artifacts.** A backup is not considered usable until it has been restored into an isolated environment and validated.

The current Kubernetes bundle uses a **single Redis StatefulSet replica with a persistent volume**. That provides persistence across pod restarts, but it is not high availability by itself. A node, volume, or cluster outage can still interrupt queue service. Production deployments should use managed Redis, Redis Sentinel/Cluster, or a replicated storage design after validating the chosen topology.

---

## 2. Recovery objectives

These are recommended initial targets. Adjust them to the actual storage and platform guarantees before production approval.

| Component | Target RPO | Target RTO | Recovery priority |
|---|---:|---:|---|
| Redis queue stream and reservations | ≤ 5 minutes with scheduled backups | ≤ 30 minutes | P0 |
| Admission controller API | 0 data loss; stateless restart | ≤ 10 minutes | P0 |
| Rollout state and audit metadata | ≤ 5 minutes | ≤ 30 minutes | P0 |
| Episode event histories | 0 loss for committed events | ≤ 60 minutes | P0 |
| Grafana dashboards | ≤ 24 hours | ≤ 2 hours | P2 |
| Prometheus time series | Best effort | ≤ 4 hours | P2 |

### Recovery priority order

1. Stop new admissions if state is uncertain.
2. Protect and verify committed event histories.
3. Restore Redis and queue metadata.
4. Restore one healthy admission-controller instance.
5. Reconcile reservations and in-flight episodes.
6. Restore remaining controller replicas.
7. Resume canary traffic.
8. Validate gate behavior and publication controls.

---

## 3. Protected data inventory

### Redis data

Redis contains or may contain:

- durable episode queue stream: `molt:episodes`;
- consumer-group state and pending entries;
- admission reservations and slot leases;
- worker health and circuit-breaker state;
- rollout candidate and previous-good configuration;
- queue transition audit metadata;
- idempotency keys and execution-attempt records;
- reconciliation markers.

Redis persistence is configured with AOF and periodic RDB snapshots. AOF `everysec` reduces loss exposure but does not guarantee zero data loss during total storage failure.

### Data that must not rely solely on Redis

Redis should not be the only copy of:

- signed event histories;
- final gate reports;
- human-review decisions;
- completed episode packages;
- deployment and rollout decisions.

Those artifacts must also be written to durable object storage, a versioned database, or another backup target.

### Admission controller state

The admission controller should be stateless between requests. Persistent state belongs in Redis or durable artifact storage. Each replica must be able to restart without inventing reservations or assuming that in-memory state is authoritative.

---

## 4. Roles and incident command

| Role | Responsibility |
|---|---|
| Incident commander | Declares severity, coordinates decisions, authorizes failover and recovery completion |
| Queue operator | Freezes admissions, inspects Redis streams, reconciles reservations |
| Platform operator | Restores StatefulSet, storage, services, replicas, and network policy |
| Integrity operator | Verifies signed histories, event ordering, idempotency, and gate behavior |
| Communications owner | Records timeline, status, impact, and recovery updates |

During a P0 event, one person may hold multiple roles, but the incident commander must explicitly record who owns each function.

---

## 5. Severity classification

### P0 — Data integrity or total admission outage

Examples:

- Redis unavailable and no verified failover exists.
- Signed-history verification failure.
- Duplicate or out-of-order event detected.
- All admission-controller replicas unavailable.
- Gate unavailable while publish requests are active.

Action: freeze admissions and publication immediately; begin recovery procedure.

### P1 — Degraded capacity or partial failover

Examples:

- One controller replica unavailable while another remains healthy.
- Redis pod restarting but PVC intact.
- One worker pool unhealthy with another available.
- Queue age exceeds the hard SLO but data remains accessible.

Action: maintain safe operation if integrity is verified; repair within the target RTO.

### P2 — Monitoring or non-critical control-plane issue

Examples:

- Grafana unavailable.
- Prometheus scrape gap.
- Dashboard ConfigMap not refreshed.

Action: restore observability without interrupting verified queue processing unless the issue affects incident response.

---

## 6. Preventive controls

### Required controls

- Persistent volume with a durable StorageClass.
- Redis AOF enabled with `appendfsync everysec`.
- Scheduled RDB and AOF backups copied outside the cluster.
- Backup encryption and access control.
- Backup retention policy with daily, weekly, and monthly points.
- Admission-controller replicas spread across nodes when possible.
- PodDisruptionBudgets enabled.
- Readiness and liveness probes enabled.
- Versioned rollout configuration with previous-good pointer.
- Synthetic queue and gate regression tests.
- Immutable logs for failover and reconciliation decisions.

### Recommended Kubernetes hardening

Add when the cluster supports it:

- `topologySpreadConstraints` for admission-controller replicas;
- anti-affinity between controller replicas;
- a dedicated StorageClass with snapshot support;
- a separate node pool for stateful workloads;
- NetworkPolicy restricting Redis access to the admission controller and backup job;
- External Secrets or Sealed Secrets instead of committed plaintext Secret values;
- image digests rather than mutable tags;
- a managed Redis-compatible service for production HA.

---

## 7. Backup procedure

### Backup types

Maintain all of the following:

1. **Redis logical backup:** `redis-cli --rdb` or equivalent snapshot.
2. **Redis AOF copy:** consistent copy of the AOF file after coordinating with the Redis process.
3. **PVC snapshot:** CSI VolumeSnapshot where supported.
4. **Application export:** queue entries, reservations, rollout state, and audit records exported to JSONL or object storage.
5. **Episode artifact backup:** signed events, raw transcripts, gate reports, review decisions, and source metadata.

### Backup schedule

Recommended starting schedule:

| Artifact | Frequency | Retention |
|---|---:|---:|
| Redis logical snapshot | Every 15 minutes | 24 hours |
| PVC snapshot | Hourly | 7 days |
| Full application export | Hourly | 30 days |
| Daily full backup | Daily | 90 days |
| Monthly archive | Monthly | 12 months |

### Backup checklist

For every backup:

- record timestamp and source pod;
- record Redis version and deployment version;
- record backup checksum;
- record queue depth and pending count;
- record latest stream IDs;
- record current rollout ID and config version;
- encrypt before off-cluster transfer;
- verify object storage upload;
- emit a backup success metric;
- periodically restore the backup into an isolated namespace.

### Example backup commands

The exact commands depend on the backup image and storage provider. The following illustrates the operator flow:

```bash
NS=molt-show
POD=$(kubectl -n "$NS" get pod -l app.kubernetes.io/name=molt-queue -o jsonpath='{.items[0].metadata.name}')
STAMP=$(date -u +%Y%m%dT%H%M%SZ)

kubectl -n "$NS" exec "$POD" -- redis-cli \
  -a "$REDIS_PASSWORD" --rdb "/data/dump-$STAMP.rdb"

kubectl -n "$NS" cp "$POD:/data/dump-$STAMP.rdb" \
  "./backup/$STAMP/dump.rdb"

sha256sum "./backup/$STAMP/dump.rdb" > "./backup/$STAMP/dump.rdb.sha256"
```

Do not treat a file copied from the pod as an off-cluster backup until it has been copied to independent durable storage.

---

## 8. Backup restore validation

A backup is approved for disaster recovery only after this test passes:

1. Create an isolated namespace, for example `molt-restore-test`.
2. Restore the PVC snapshot or Redis dump into an isolated Redis instance.
3. Start a read-only reconciliation tool.
4. Compare:
   - stream lengths;
   - latest stream IDs;
   - pending consumer entries;
   - reservation counts;
   - rollout state;
   - audit record counts;
   - idempotency-key counts;
   - checksums of exported artifacts.
5. Run one synthetic episode with writes redirected to a test event store.
6. Verify the signed event history.
7. Run the gate regression fixtures and confirm expected pass/reject behavior.
8. Record restore duration and data loss window.

A failed restore validation must page the platform owner and invalidate the backup for recovery use.

---

## 9. Admission-controller replica failure

### Symptoms

- One pod is `CrashLoopBackOff`, `NotReady`, or unavailable.
- Service endpoints fall from two to one.
- `molt_admission_controller` health metrics show a replica gap.
- Requests receive 5xx or timeout responses.

### Immediate response

1. Confirm at least one healthy replica is serving:

```bash
kubectl -n molt-show get deploy,pods,endpoints molt-admission-controller
kubectl -n molt-show rollout status deployment/molt-admission-controller
```

2. Do not change global admission limits while diagnosing.
3. Confirm Redis connectivity from the healthy replica.
4. Inspect the failed pod:

```bash
kubectl -n molt-show describe pod <pod>
kubectl -n molt-show logs <pod> --previous
```

5. If one replica remains healthy and reservations are stored centrally in Redis, continue at the last-known-good capacity.
6. If both replicas are unavailable, execute the total admission outage procedure.

### Recovery

- Fix image, configuration, secret, or node issue.
- Restart or replace the failed pod.
- Wait for readiness and Redis health checks.
- Run a synthetic enqueue/dequeue smoke test.
- Verify no duplicate reservations were created.
- Confirm metrics return to the expected replica count.

Do not manually edit Redis reservations unless the reconciliation procedure below has been run and the change is recorded.

---

## 10. Redis pod restart with PVC intact

### Symptoms

- Redis pod restarts or becomes unready.
- PVC remains attached and healthy.
- Admission controller reports Redis connection errors.

### Procedure

1. Freeze new admissions:

```bash
kubectl -n molt-show scale deployment/molt-admission-controller --replicas=0
```

2. Check pod, PVC, and events:

```bash
kubectl -n molt-show get pod,pvc,events --sort-by=.lastTimestamp
kubectl -n molt-show describe pvc data-molt-queue-0
```

3. Confirm the PVC is not mounted by an unexpected pod.
4. Allow the StatefulSet to restart or delete only the failed pod if the controller is not recovering it.
5. Wait for Redis readiness.
6. Validate stream and reservation counts.
7. Run reconciliation.
8. Scale the admission controller back to its configured replica count.
9. Run a synthetic episode and history verification.

### Success criteria

- Redis accepts authenticated connections.
- The latest stream ID is present.
- No unexpected decrease in queue or audit counts.
- No orphaned slot reservations remain.
- Admission controller readiness passes.
- Synthetic event history verifies.

---

## 11. Redis PVC or node failure

This is a P0/P1 event depending on backup availability.

### Immediate containment

1. Freeze admission and publication.
2. Scale admission-controller replicas to zero or set the intake endpoint to maintenance mode.
3. Preserve all controller logs and active episode metadata.
4. Record the last known Redis stream ID and backup timestamp if available.
5. Do not delete the original PVC until the incident commander approves it.

### Preferred recovery order

1. Reattach the original PVC on a replacement node.
2. If impossible, restore the latest verified PVC snapshot.
3. If unavailable, restore the latest verified Redis logical backup.
4. If Redis backup is unavailable, restore application exports and mark the missing interval as an explicit recovery gap.

### Restoration

```bash
kubectl -n molt-show get pvc
kubectl -n molt-show get volumeattachment
kubectl -n molt-show describe pvc data-molt-queue-0
```

If restoring into a replacement StatefulSet:

- use the same Redis major/minor version;
- preserve the same password and configuration policy;
- restore AOF/RDB before admitting traffic;
- verify the restored stream IDs and counts;
- keep the old PVC read-only or isolated for forensic comparison;
- never run two independent writable Redis primaries for the same queue.

### Recovery point declaration

If the restored backup is older than the target RPO, record:

- backup timestamp;
- last known stream ID;
- estimated missing interval;
- number of episodes potentially affected;
- whether any event histories were committed after the backup.

Those episodes must be reconciled from application artifacts before resuming normal admission.

---

## 12. Total admission outage procedure

Use when no healthy admission-controller replica can serve requests.

1. Declare P0.
2. Disable or remove the intake route so producers receive an explicit maintenance response.
3. Scale the controller to zero if a crash loop is causing repeated load.
4. Verify Redis health directly.
5. Restore one controller replica only; do not immediately restore all replicas.
6. Run `/healthz` and `/readyz` checks.
7. Confirm policy ConfigMap and Secret versions.
8. Run a synthetic queue submission using a dedicated test idempotency key.
9. Confirm the test reservation is released exactly once.
10. Run a synthetic signed-history verification and gate regression test.
11. Restore the second replica.
12. Resume 5% canary intake.
13. Monitor two healthy windows before returning to normal traffic.

The controller must fail closed if it cannot establish Redis connectivity or cannot determine whether a reservation already exists.

---

## 13. Queue and reservation reconciliation

Reconciliation is required after Redis restoration, controller split-brain suspicion, forced pod termination, or any backup restore.

### Reconciliation inputs

- Redis stream entries;
- consumer-group pending entries;
- reservation keys and leases;
- worker health registrations;
- controller audit events;
- episode event histories;
- gate reports;
- durable application exports;
- controller logs from the incident window.

### Reconciliation rules

| Condition | Action |
|---|---|
| Queued episode with no reservation | Requeue normally |
| Reservation with healthy running worker | Keep reservation and observe |
| Reservation with dead worker and no committed event | Release and create retry attempt |
| Reservation with partial signed history | Quarantine; do not silently resume |
| Completed episode with remaining reservation | Release reservation and audit correction |
| Duplicate idempotency key | Keep earliest valid attempt; quarantine later attempt |
| Unknown worker ID | Mark reservation orphaned and hold for review |
| Stream entry older than max queue age | Mark expired and notify operator |

### Reconciliation procedure

1. Stop new admissions.
2. Take a fresh Redis backup before changes.
3. Export current stream and reservation state.
4. Run the reconciliation tool in dry-run mode.
5. Review counts and proposed transitions.
6. Obtain incident-commander approval for mutations.
7. Apply transitions atomically where possible.
8. Emit an immutable reconciliation report.
9. Re-run the dry-run tool and confirm zero unexpected discrepancies.
10. Resume canary admission only after integrity checks pass.

Never use `FLUSHALL`, delete the stream, or bulk-delete reservations as a recovery shortcut.

---

## 14. Failover to a secondary cluster or managed Redis

A secondary environment must be provisioned before it is needed. It should contain:

- compatible Redis version and persistence settings;
- admission-controller image and configuration;
- worker registrations or a safe empty worker pool;
- gate binary and regression fixtures;
- access to durable event artifacts;
- monitoring and alerting;
- secrets from the approved secret manager;
- DNS or service-routing mechanism with controlled TTL.

### Planned failover

1. Freeze new admissions in the primary.
2. Allow running episodes to finish or explicitly quarantine them.
3. Capture a final Redis backup and stream ID.
4. Replicate or restore the backup to the secondary.
5. Verify counts, stream IDs, reservations, and rollout state.
6. Start one admission-controller replica in the secondary.
7. Run smoke tests and gate regression tests.
8. Shift 5% of intake to the secondary.
9. Observe one healthy window.
10. Shift the remaining traffic.
11. Restore the second controller replica.
12. Keep the primary read-only until reconciliation is complete.

### Unplanned failover

If the primary is unavailable:

1. Declare P0 and freeze the primary route if possible.
2. Identify the latest verified secondary backup.
3. Record the expected RPO gap.
4. Restore Redis into the secondary.
5. Start one controller replica in failover mode.
6. Disable automatic retries until idempotency state is verified.
7. Run reconciliation from durable episode artifacts.
8. Resume with a 5% canary.
9. Keep publication blocked until signed histories and gate state are verified.
10. Scale to normal traffic only after two healthy windows.

### Split-brain prevention

Only one environment may be writable for a given queue namespace. Use one of:

- a global traffic lock;
- a lease in an external quorum system;
- managed database failover fencing;
- operator-approved promotion token.

DNS changes alone are not sufficient split-brain protection.

---

## 15. Monitoring during recovery

Watch these panels continuously:

- Redis readiness and authenticated connection count;
- StatefulSet pod and PVC status;
- admission-controller replica readiness;
- queue depth and p95 queue age;
- pending consumer entries;
- reservation count and orphaned reservations;
- episode completion and timeout rate;
- signed-history verification failures;
- duplicate or out-of-order event counters;
- gate availability and verdict distribution;
- backup age and last successful restore test;
- rollout stage and candidate/previous-good configuration.

### Recovery-specific alerts

Page immediately on:

- any signed-history verification failure;
- any duplicate or event-order violation;
- more than one writable Redis primary;
- Redis backup age beyond the RPO;
- no healthy admission-controller replica;
- queue state count mismatch after restore;
- gate unavailable during recovery;
- repeated reservation reconciliation mutations.

---

## 16. Disaster exercises

Run these exercises at least quarterly and after major architecture changes.

### Exercise A — Controller pod loss

- Delete one controller pod.
- Confirm the other replica continues serving.
- Verify no duplicate reservations.
- Confirm the replacement becomes ready.

### Exercise B — Redis pod restart

- Freeze admissions.
- Restart the Redis pod with PVC intact.
- Validate AOF recovery and stream continuity.
- Reconcile reservations.

### Exercise C — PVC restore

- Restore the latest snapshot into an isolated namespace.
- Compare stream IDs and counts.
- Run synthetic event and gate tests.
- Measure restore time.

### Exercise D — Admission controller total outage

- Stop all controller replicas.
- Confirm intake fails closed.
- Restore one replica and run smoke tests.
- Restore the second replica and resume canary traffic.

### Exercise E — Secondary-cluster failover

- Freeze primary intake.
- Restore the latest backup to the secondary.
- Promote the secondary with a fencing token.
- Run reconciliation and canary traffic.
- Measure RTO and declared RPO.

### Exercise F — Corrupt or stale backup

- Present an intentionally invalid or stale backup.
- Confirm restore validation rejects it.
- Confirm no traffic is promoted to the invalid restore.

Every exercise must produce a report containing:

- start and end timestamps;
- operator names;
- commands or automation version;
- RPO achieved;
- RTO achieved;
- episodes affected;
- reconciliation mutations;
- integrity-test results;
- follow-up actions.

---

## 17. Recovery completion checklist

The incident commander may close the recovery only when all items are true:

- [ ] One and only one writable queue primary is active.
- [ ] Redis authentication and readiness checks pass.
- [ ] PVC or restored backup checksum is recorded.
- [ ] Latest stream ID and queue counts are recorded.
- [ ] Pending entries and reservations are reconciled.
- [ ] No orphaned reservations remain without an explicit decision.
- [ ] Admission-controller replicas are ready and balanced.
- [ ] Synthetic enqueue/dequeue test passes.
- [ ] Signed-history verification passes for recovery test episodes.
- [ ] Gate regression fixtures return expected verdicts.
- [ ] No duplicate or out-of-order event violations occurred.
- [ ] Queue age and completion rate return within SLO.
- [ ] Monitoring and alerts are active.
- [ ] Rollout is held or returned to the previous-good configuration.
- [ ] Incident timeline, RPO, RTO, and data-loss assessment are documented.
- [ ] Follow-up tasks have owners and due dates.

---

## 18. Post-incident review

Within two business days, document:

- root cause and contributing factors;
- time to detect;
- time to contain;
- time to restore Redis;
- time to restore the first controller replica;
- time to resume canary traffic;
- actual RPO and RTO;
- queue entries lost, duplicated, expired, or retried;
- signed histories affected;
- gate and publication impact;
- backup and monitoring effectiveness;
- changes required to manifests, alerts, or automation.

A recovery incident is not complete until the runbook, manifests, dashboards, and backup policy are updated to reflect what actually happened.
