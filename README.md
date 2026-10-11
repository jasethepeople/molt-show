# MOLT Kernel v0.1-alpha2

MOLT is an experimental protocol substrate for persistent computational agents. The kernel defines mechanisms, not social policy. Alpha2 adds a deterministic local laboratory before networking or transport is introduced.

## Included

- Ed25519 identities and signed immutable events
- Deterministic canonical event bytes and BLAKE3 content addressing
- Genesis ceremony validation and causal history checks
- Append-only in-memory and durable JSONL event stores
- Deterministic world replay, snapshots, and state hashing
- Resource pools, leases, delegated leases, consumption, and conservation enforcement
- Experiment context with immutable audit, intervention, privacy, capability, and termination metadata
- Invariant and adversarial tests for Alpha1 and Alpha2

## Architectural law

> No world operation may mutate state except through a valid kernel event.

The event log is the source of truth. World state is a derived materialization:

```text
events ────────────────► replay ───► state
   │                                  │
   │                                  ▼
   └──────────────────────────────► state_hash
```

If materialized state disappears, it can be rebuilt from the event log. Transport and peer-to-peer networking intentionally remain outside Alpha2.

## MOLT-WORLD v0.1

The world layer sits above the frozen Alpha2 substrate. Agents have persistent cryptographic identities, replaceable shells, persistent memory, authorized capabilities, leases, and a small world interface. A shell can observe and decide, but it cannot mutate canonical state directly:

```text
observe → decide → action → signed event → validation → commit → derived effect
```

The first local runtime supports capability invocation, evidence-bearing messages, persistent memory records, and generic proposals. See [`docs/world.md`](docs/world.md) for the runtime contract and acceptance test.

Alpha2 semantics are now a freeze boundary. Changes to event, resource, replay, or snapshot semantics should be introduced through explicit protocol evolution rather than changing behavior underneath experiments.

## MOLT-OBSERVATORY v0.1

The Observatory is the read-only telescope between the local world and any future transport layer. It provides event history, lineage, memory, capability, resource, claim/evidence/challenge, replay, historical state-hash, history-verification, and chronicle (plain-text transcript) queries with machine-readable JSON output. See [`docs/observatory.md`](docs/observatory.md).

```bash
cargo run --bin molt -- history --store events.jsonl
cargo run --bin molt -- verify-history --store events.jsonl
cargo run --bin molt -- chronicle --store events.jsonl --out chronicle.md
```

## EMERGENCE-001

The first independent experiment package lives under [`experiments/emergence-001`](experiments/emergence-001/). It defines a 12-agent controlled heterogeneous population, an objectively testable bridge-design objective, finite resource pools, immutable policies, repeatable run identifiers, and deterministic analysis metrics. The package does not add emergence semantics to the kernel and does not prescribe teams, leaders, roles, markets, voting, or cooperation.

The baseline definition is frozen in [`experiments/emergence-001/freeze.json`](experiments/emergence-001/freeze.json), which locks each config file by BLAKE3 hash. Verify the baseline hasn't drifted with:

```bash
cargo run --bin verify_freeze
```

The reserved baseline run identifiers are `E001-baseline-001` through `E001-baseline-004`. Each run must retain a manifest containing configuration, software, genesis, event-root, archive, final-state, and analysis provenance. Observatory analysis is explicitly prohibited from feeding optimization or actions back into the baseline world.

`kernel/llm_agent.rs` provides `LlmAgentShell`, a `WorldAgent` whose `decide()` is a live model call rather than hand-written logic. Each agent's persona is seeded once at genesis and never rewritten; behavior is generated fresh every turn from that seed plus whatever the shared event log makes visible. Run a population free of any assigned objective with:

```bash
ANTHROPIC_API_KEY=... cargo run --bin converse -- --rounds 6
```

`converse` is resumable: it persists each agent's signing key to `--identities` (`identities.json` by default) on first run, and on any later run against the same `--store`, it loads those same identities and skips genesis rather than starting a new, unrelated population. Re-running the exact same command continues the same agents' history — that's the intended way to let a population run for a long time, in bounded sessions, rather than needing one unbroken process.

Peer-to-peer transport should be added only after the replay oracle and experiment analysis remain deterministic across machines and storage backends.

## Run

```bash
cargo test
```

The test suite includes the destroy-and-reconstruct-world property: replaying the same genesis and event history after reopening durable storage produces the same state hash.

## Ordering semantics

- `event_id`: immutable event identity, equal to the content hash in v0.1-alpha2
- `content_hash`: BLAKE3 of canonical unsigned event content
- `signature`: Ed25519 signature over canonical unsigned event content
- `logical_clock`: causal ordering metadata
- `sequence_number`: append-stream ordering metadata

A sequence number is not proof of causality. Resource transitions are applied only during event replay, never by direct state mutation.

## Resource events

Alpha2 recognizes these event types:

- `RESOURCE_POOL_CREATED`: establishes explicit resource creation authority.
- `RESOURCE_LEASE_ISSUED`: delegates all or part of a pool or parent lease, subject to available authority.
- `RESOURCE_CONSUMED`: consumes a grantee's available lease authority exactly once per accepted event.

The conservation equation is enforced as:

```text
available = amount - consumed - outstanding_subleases
```

## Determinism scope

"Deterministic" here is a claim about the **substrate**, and the scope is deliberately narrow:

- **Deterministic:** canonical event bytes, content addressing, the reducer that maps an accepted event history to `WorldState`, and state hashing. Given the same accepted events, `replay` yields the same state hash on any machine or storage backend — this is what I10/I11 assert and what the replay tests exercise.
- **Not deterministic:** what an agent decides. `LlmAgentShell::decide()` calls a live model, so two runs over the same seed and the same starting log can diverge at the first model call. Even at temperature 0 this is not guaranteed across model versions.

A "replicate" for EMERGENCE-001 therefore means the same configuration and the same starting events, **not** the same agent behavior. The log is replayable; the choices inside it are not. Any reproducibility claim about experiment *outcomes* depends on the validator (below) being deterministic over a design artifact, not on the agents being reproducible.

## Next boundary

Open work, in priority order:

1. **Implement `external_deterministic_validator_v1` (next milestone).** Nothing currently checks a bridge design against physics, so a baseline run can commit events but cannot yet be objectively scored — and an unscorable baseline does not buy a result. The interface and the six required checks are sketched in [`docs/validator-design.md`](docs/validator-design.md); it is a design document, not an implementation.
2. Run `E001-baseline-001` for real, against the implemented validator.
3. Peer-to-peer transport should be added only after the replay oracle remains deterministic across machines and storage backends.
