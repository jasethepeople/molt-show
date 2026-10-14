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
- `gate-greenbrier-unsourced.json`: regression fixture that must be rejected
- `gate-greenbrier-cited.json`: regression fixture that may pass

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
  --backend ollama \
  --population experiments/show-001/population.json \
  --roles experiments/show-001/roles.json \
  --models experiments/show-001/models.json \
  --question "Should cities prioritize expanding public transit or improving roads when budgets are limited?" \
  --rounds 6
```

`models.json` maps Shell-A through Shell-D to different Ollama models. Use
`--model MODEL_NAME` to override the map for a controlled comparison. Set
`OLLAMA_BASE_URL` when Ollama runs on another host; otherwise the runner uses
`http://127.0.0.1:11434`. The existing Anthropic path remains available with
`--backend anthropic`, and `--backend mock` is useful for zero-cost wiring
tests.

`--question` is for a fresh run only. It appends a signed
`VIEWER_QUESTION_POSTED` event and exposes the question to every agent as
quoted viewer data rather than as a system instruction.

## Pre-publication gate

The standalone gate is outside agent control. It consumes a JSON proposal and
returns `pass`, `review`, or `reject` with machine-readable findings:

```bash
cargo run --bin gate -- \
  --input experiments/show-001/gate-greenbrier-unsourced.json \
  --output /tmp/gate-report.json \
  --audit /tmp/gate-audit.jsonl \
  --review-queue /tmp/human-review.jsonl
```

The deterministic v1 checks cover:

- missing AI disclosure
- quantified or named factual claims without inline `[source:ID]` citations
- unknown, non-HTTPS, or weakly matching source records
- unsupported factual-sounding language routed to human review
- internal capability or prompt leakage
- phone-like personal data

The Greenbrier-style `24%` claim is rejected without a citation. A cited claim
is only eligible to pass when its source record uses HTTPS and its support text
matches the claim. The gate does not claim that a source is truthful; it checks
traceability and routes uncertain cases to review.

Every evaluation can append the machine-readable report to an audit JSONL
stream. Any `review` or `reject` verdict is appended to the human-review queue
with status `pending_human_review`. Both verdicts return a non-zero exit status,
so a producer or CI pipeline cannot accidentally publish a non-pass proposal.
Only `pass` exits successfully.

Pending items can be inspected and decided without mutating the original
queue:

```bash
cargo run --bin review -- list --queue /tmp/human-review.jsonl
cargo run --bin review -- decide \
  --queue /tmp/human-review.jsonl \
  --decisions /tmp/review-decisions.jsonl \
  --target show-001-episode-002-raw-draft \
  --reviewer producer-human-01 \
  --decision revise \
  --note "Attach sources and remove internal terminology."
```

Valid decisions are `approve`, `reject`, and `revise`. Decisions are append-only
records, preserving the original gate result and the reviewer identity.

## Invariants

- The event log remains the source of truth.
- Agent actions become signed events before they affect the world.
- Viewer text is treated as quoted data, never as system instructions.
- Real-world claims require source tracking or an explicit uncertainty label.
- No content is published without human review in the first release.
