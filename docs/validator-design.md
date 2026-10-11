# `external_deterministic_validator_v1` — interface sketch

> **STATUS: NOT IMPLEMENTED.** This document is a design sketch only. There is no
> code behind it; `objective.json` names the validator, but nothing in this
> repository checks a bridge design against physics. A baseline run today can
> commit events but cannot be objectively scored.

## Why it is the next milestone

Everything upstream of scoring — 12 agents, resource pools, immutable policies,
deterministic analysis metrics — is infrastructure for an experiment whose
*outcome* is undefined until a design can be graded. Running `E001-baseline-001`
before the validator exists produces a log, not a result.

## Contract

The validator is a pure function from a **design artifact** to a
**deterministic verdict**. Given the same artifact bytes it MUST return the same
verdict, byte-for-byte, on any machine — this is what makes a bridge-design
*outcome* reproducible even though agent *behavior* is not (see the determinism
scope note in `README.md` / `docs/world.md`).

```text
artifact (canonical bytes)  ──►  validator_v1  ──►  verdict
                                                    ├─ artifact_hash  (BLAKE3)
                                                    ├─ checks:  {name -> pass|fail}
                                                    ├─ score:   float (derived ONLY from checks)
                                                    └─ evidence_bundle_present: bool
```

### Input

An artifact must be self-contained and canonically serialized (same
canonicalization as events: sorted keys, no floats where integers suffice) so
that `artifact_hash` is stable:

- geometry: node/segment graph of the span (units in metres, SI)
- materials: per-member material id + section properties
- loads: named load cases with magnitudes and points of application
- claimed evidence refs: ids of `EVIDENCE`-kind events the design cites

### Required checks (mirror `objective.json` → `validation.required`)

| check id | meaning |
|---|---|
| `span_geometry_valid` | geometry is well-formed and actually spans 100 m within tolerance |
| `load_case_passes` | internal forces under each load case stay within member capacity |
| `material_limits_pass` | stress/strain within declared material allowables |
| `deflection_limit_passes` | serviceability deflection under span-scaled limit |
| `stability_checks_pass` | buckling / overturning / global stability |
| `evidence_bundle_present` | the referenced evidence events exist and are non-empty |

`score` is a deterministic function of the check vector only (e.g. weighted
pass fraction with fixed weights declared in the validator version) — never of
wall-clock time, RNG state, or agent identity.

### Failure modes to define explicitly

- **malformed artifact** → verdict is a structured `rejected` with a reason, not
  a crash (a crashing validator is not a deterministic oracle).
- **unknown material/section id** → fail the dependent checks, keep the rest.
- **float determinism** → fix a numeric kernel (e.g. deterministic linear solve
  with defined tolerance and rounding of reported values) so verdicts do not
  flip on a different libm.

### How it plugs in (proposed)

Not decided yet; two candidate shapes, both keeping the architecture law intact:

1. **Observatory-side, offline.** A new read-only command reconstructs the
   design artifact from the event log and runs the validator; the verdict is
   written next to the run manifest. Nothing is fed back into the world, which
   matches `freeze.json`'s "analysis must not feed optimization back".
2. **In-world, as an event.** The validator result becomes a signed
   `VALIDATION_RESULT` event committed by a distinct evaluator identity, so the
   score is part of the auditable log. This requires a protocol-evolution note
   because it adds an event type to a frozen-semantics boundary.

Open questions for the author: which of the two, whether the evaluator is one of
the 12 agents or an external attester, and whether `score` is on the log at all
given the no-feedback constraint.
