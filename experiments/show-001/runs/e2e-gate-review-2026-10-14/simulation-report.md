# Show-001 End-to-End Gate + Human Review Simulation

- **Simulation:** `e2e-gate-review-2026-10-14`
- **Input:** five-round live transcript from `second-live-2026-10-13`
- **Viewer question:** Should cities prioritize expanding public transit or improving roads when budgets are limited?

## Flow result

| Stage | Result |
|---|---|
| Signed live transcript verification | `valid: true`, 61 events |
| Raw transcript gate | **REJECT** |
| Human review queue | 1 item queued |
| Human decision | **REVISE** by `producer-human-01` |
| Corrected sourced draft gate | **PASS** |
| Audit records | 2 |

## Raw transcript gate findings

- Sentences checked: **970**
- Cited sentences: **0**
- Total findings: **651**
- Severity counts: `{'reject': 631, 'review': 20}`
- Top finding codes: `{'unsourced_factual_claim': 628, 'unsupported_factual_language': 20, 'missing_ai_disclosure': 1, 'capability_leakage': 1, 'personal_data': 1}`

The large finding count is expected because the raw chronicle is an internal transcript, not a publication-ready script. It includes timestamps, agent labels, repeated unsourced claims, and capability terminology. The gate correctly prevented it from crossing the publication boundary.

## Human review decision

`producer-human-01` recorded **revise** with this instruction:

> Remove unsupported transcript claims, remove internal capability language, and attach source records before publication.

The original queue JSONL remains immutable; the decision is appended separately as an audit record.

## Corrected draft gate

The revised proposal used one sourced, audience-facing claim and an explicit AI-run disclosure:

> Cities should compare public transit and road investments using local ridership, safety, maintenance, and access data [source:DOT1]. This show is AI-run.

The corrected draft returned **pass**, exited with status `0`, and was not added to the human-review queue.

## Conclusion

**End-to-end control flow passed:** signed show transcript → deterministic gate → review queue → human revision decision → corrected sourced draft → gate pass.

This simulation demonstrates publication blocking and traceable remediation. It does not establish that the source itself is truthful; source truth still requires research-worker retrieval and human fact-checking.
