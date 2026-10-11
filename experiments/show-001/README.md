# MOLT Show-001

This experiment adapts the MOLT world runtime into a 12-agent AI panel for a viewer-question show.

## Relationship to EMERGENCE-001

`emergence-001` remains the frozen Alpha2 baseline experiment and is not modified by this package. `show-001` is a separate experiment with show-specific roles, policies, and services layered above the same kernel.

The panel intentionally contains **12 agents**. The earlier proposal for 14 agents was a planning mistake; 12 is the authoritative population size for this build.

## Current package

- `population.json`: the 12-agent population inherited from the kernel baseline
- `roles.json`: show-specific roles and behavior guidance
- `capabilities.json`: show permissions and capability authorization
- `models.json`: explicit shell-to-local-model assignments and fallback model

## Planned additions

The build will add these components outside the frozen kernel semantics:

1. viewer-question queue and sanitizer
2. intake screener
3. research and answer-package events
4. episode proposal and producer service
5. pre-publication guardrail gate
6. human review queue
7. canon compiler and chronicle publishing
8. render pipeline for audio, captions, and visuals

## Phase 0 measurement

The first chronicle should measure whether the panel is too agreeable or too
hedged. In particular, inspect the Contrarian and Wildcard for useful
disagreement and inspect the Host and Editor for whether debate converges to a
clear answer. If the show is dull, sharpen those two roles before adding more
agents.

Run the show runner with explicit paths so it cannot accidentally use the
emergence baseline:

```bash
cargo run --bin converse -- \
  --population experiments/show-001/population.json \
  --roles experiments/show-001/roles.json \
  --rounds 6
```

## Invariants

- The event log remains the source of truth.
- Agent actions become signed events before they affect the world.
- Viewer text is treated as quoted data, never as system instructions.
- Real-world claims require source tracking or an explicit uncertainty label.
- No content is published without human review in the first release.
