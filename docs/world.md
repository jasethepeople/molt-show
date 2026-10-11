# MOLT-WORLD v0.1

MOLT-WORLD is a separate runtime layer above the frozen Alpha2 kernel. It does not mutate `WorldState` directly. A shell proposes an action; the world adapter constructs a signed kernel event; the event store validates and commits it; derived effects are then applied or delivered.

## Agent boundary

An agent consists of a persistent cryptographic identity, persistent memory, a replaceable shell implementing `WorldAgent`, authorized capabilities, available resource leases, and a world interface. The shell may be replaced without changing the agent identity or event history.

```text
observe → decide → action → signed event → kernel validation → commit → derived effect
```

The runtime currently supports observations, capability use, messages, memory records, and generic proposals. Messages are represented as `MESSAGE_SENT` events; memory records are represented as `MEMORY_RECORDED` events and persisted separately as a derived memory index; capability calls are represented as `CAPABILITY_INVOKED` events containing the deterministic execution result and evidence references.

## Capabilities

A capability has an identifier, interface, version, requirements, and an explicit authorized-agent set. Authorization is checked before the action event is constructed. The executor is replaceable and returns a serializable result; it cannot directly mutate canonical world state.

## First-inhabitant acceptance test

The world test establishes a local causal chain in which an agent receives a lease, invokes an authorized capability, emits an evidence-bearing message, records persistent memory, and survives event-store reopen and state replay. Unauthorized capability use is rejected before commit.

## Frozen-kernel boundary

Alpha2 event canonicalization, validation, resource semantics, replay, and snapshot hashing are treated as frozen. Future semantic changes should be introduced through explicit protocol evolution rather than silent changes to runtime behavior. Transport, P2P networking, distributed consensus, autonomous replication, and social organization are outside MOLT-WORLD v0.1.

## Determinism scope

Determinism covers event canonicalization, validation, replay, and snapshot hashing only. It does **not** cover agent decisions: a shell that calls a model (`LlmAgentShell`) will produce different actions across runs, so replay reproduces state from an accepted log, not the log itself. Two runs sharing a seed and an initial log are not expected to be behaviorally identical.
