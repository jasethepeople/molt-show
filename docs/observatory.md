# MOLT-OBSERVATORY v0.1

The Observatory is a read-only scientific instrument over canonical event history and derived state. It has no mutation authority: every result is reconstructed from the event store or persistent memory index.

## Query surface

| Query | Purpose |
|---|---|
| `history` | Return canonical events, optionally filtered by agent. |
| `events --type TYPE` | Filter events by semantic type. |
| `lineage EVENT_ID` | Reconstruct ancestors and descendants from causal parents. |
| `memory --agent ID` | Inspect persistent memory records. |
| `capabilities` | Inspect capability invocation history. |
| `resources` | Reconstruct pools and leases from replayed state. |
| `claims`, `evidence`, `challenges` | Inspect epistemic provenance events. |
| `replay EVENT_ID` | Reconstruct world state at a historical event. |
| `state-hash EVENT_ID` | Compute the historical state hash at an event. |
| `verify-history` | Validate event integrity, causal parents, and stream order. |

All commands emit JSON. Use `--store PATH` and `--memory PATH` to select durable JSONL sources; use `--agent ID`, `--type TYPE`, and `--kind KIND` for filters.

```bash
cargo run --bin molt -- history --store events.jsonl
cargo run --bin molt -- events --type MESSAGE_SENT --store events.jsonl
cargo run --bin molt -- lineage EVENT_ID --store events.jsonl
cargo run --bin molt -- resources --agent A17 --store events.jsonl
cargo run --bin molt -- replay EVENT_ID --store events.jsonl
cargo run --bin molt -- state-hash EVENT_ID --store events.jsonl
cargo run --bin molt -- verify-history --store events.jsonl
```

## Graph boundaries

The historical graph is reconstructed from event parents. The resource graph is reconstructed from resource pool and lease transitions. The epistemic graph is represented by explicit claim, evidence, and challenge event types. The relationship graph is initially measured from message events rather than prescribed by the runtime.

The Observatory does not infer truth from memory. Memory records are observations or claims whose provenance can be inspected; their epistemic status must be established by evidence and experiments.
