# EMERGENCE-001 Analysis v0.1

`analyze.py` is a read-only deterministic analysis program. It consumes an exported canonical event JSONL file and an optional memory JSONL file; it never writes to the world or event store unless the caller explicitly selects an output path for the analysis result.

```bash
python3 analyze.py \
  --config-dir .. \
  --events run/events.jsonl \
  --memory run/memory.jsonl \
  --run-id E001-baseline-001 \
  --output run/analysis.json
```

The output records the configuration hash, input counts, interaction metrics, information/provenance counts, resource archaeology, observed organizational signals, and reproducibility metadata.

Measured dimensions include messages per agent, unique contacts, reciprocity, directed network density, network centralization, claims, evidence, challenges, capability invocations, memory persistence, delegated leases, resource event flow, and observed interaction signals. These are measurements, not objectives. The analysis does not label an interaction pattern as successful organization merely because it exists.

A valid run archive should retain:

1. The exact experiment configuration files.
2. The repeatable run identifier.
3. The canonical event JSONL archive.
4. The persistent memory JSONL archive.
5. The machine-readable analysis output.
6. The state-hash and history-verification output from the Observatory.

The baseline should be repeated independently before controlled perturbations are introduced.
