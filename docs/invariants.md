# MOLT Kernel v0.1-alpha2 — Invariants

## I1 — Identity

Accepted events MUST identify an actor whose cryptographic signature verifies.

## I2 — Immutability

Accepted event bytes are immutable. Any mutation changes the content hash and invalidates the signature.

## I3 — Causal consistency

Non-genesis events MUST reference known parents. Stream sequence numbers are monotonic within the event stream; logical clocks encode causality and are not treated as physical time.

## I4 — No double consumption

A resource allocation cannot be consumed more than once. Consumption MUST NOT exceed the grantee's available lease authority.

## I5 — Delegation constraints

Sub-leases cannot grant capabilities, scope, or quantities outside their parent lease. A parent lease's available authority is its amount minus consumption and outstanding subleases.

## I6 — Lineage authorization

A lineage operation is authoritative only when authorized by the applicable cryptographic parent identity. Unauthorized lineage claims remain historical events but are not authoritative lineage.

## I7 — Read-only observatory

The Observatory has no mutation authority over canonical history.

## I8 — Conservation

Conserved resources cannot be created by ordinary events. Creation authority must be explicit, such as a genesis record or a later protocol-defined issuance mechanism.

## I9 — Durable history

The event log is append-only and durable. Reopening a durable store MUST recover the same accepted event bytes in the same stream order.

## I10 — Deterministic replay

Given the same accepted event history, replay MUST produce the same world state independent of process restart or materialized-state loss.

## I11 — Snapshot equivalence

A snapshot's state hash MUST equal the hash obtained by replaying its event history. Snapshots are derived observations, not alternate mutation authority.

## I12 — Resource conservation

For every lease, `amount = consumed + available + outstanding_subleases`. No ordinary event may allocate or consume beyond available authority.

## I13 — Lease constraint propagation

Delegated lease scope and quantity MUST remain within the parent lease's remaining authority, recursively through the delegation graph.

## I14 — Experiment provenance

Experiment objective, participants, capabilities, resource scope, termination, and policy metadata MUST be content-addressable and immutable once committed.

## I15 — Audit/privacy policy integrity

Audit, intervention, and privacy policies are experiment-boundary metadata. Changing them requires a new signed event; privacy restricts disclosure but does not imply deletion from canonical history.
