# Show-001 Second Live Test Analysis

- **Question:** Should cities prioritize expanding public transit or improving roads when budgets are limited?
- **Run:** `second-live-2026-10-13`
- **Backend:** Ollama
- **Model:** `qwen2.5:0.5b` via `--model` override
- **Population:** 12 agents
- **Rounds:** 5
- **Events:** 61 total: 1 signed viewer-question event plus 60 message events
- **History verification:** passed

## Participation and mechanics

| Metric | Result |
|---|---:|
| Messages per agent | `{'A01': 5, 'A02': 5, 'A03': 5, 'A04': 5, 'A05': 5, 'A06': 5, 'A07': 5, 'A08': 5, 'A09': 5, 'A10': 5, 'A11': 5, 'A12': 5}` |
| Messages per round | `{0: 12, 1: 12, 2: 12, 3: 12, 4: 12}` |
| Empty replies | 0 |
| Generic filler replies | 0/60 |
| Unique reply texts | 59/60 |
| Mean characters | 1589.8 |
| Median characters | 1423.0 |
| Mean words | 226.1 |
| Mean adjacent lexical Jaccard | 0.224 |
| Recipient distribution | `{'A01': 11, 'A02': 11, 'A03': 11, 'A04': 11, 'A05': 11, 'A06': 5}` |

## Topic and behavior markers

| Marker | Count |
|---|---:|
| Transit-related terms | 453 |
| Road/automobile terms | 375 |
| Uncertainty/hedging terms | 26 |
| Agreement terms | 0 |
| Disagreement/contrast terms | 83 |
| Question marks | 2 |

## Assessment

### What passed

- Ollama completed all **60/60 agent turns** across five rounds.
- The viewer question was committed as a signed `VIEWER_QUESTION_POSTED` event before round one.
- All agents received the question through a quoted-data block; no system-prompt insertion was used.
- History verification passed with zero errors.
- A chronicle was generated from the event log.
- No paid API calls were used.

### Content findings

The question materially improved the run over the empty-context baseline: generic filler fell to 0/60, and the transcript stayed on the transportation tradeoff for all five rounds.

The panel nevertheless converged quickly toward public transit rather than sustaining a balanced debate. A05 briefly argued for roads but then included pro-transit points in the same response; A09 appropriately withheld a specific recommendation pending budget and impact data; and A12 produced a compact table-like conclusion. Several agents repeated the same public-transit recommendation across rounds instead of building on distinct evidence.

There is also a clear safety/quality failure that the future gate must catch: A03 introduced a specific claim about “the Greenbrier in Virginia” and a 24% ridership increase without a source. Other outputs made broad claims about costs, pollution, safety, and economic growth without evidence. The transcript also shows capability leakage, including references to `post_message`, which should not appear in audience-facing copy.

The low-level metrics should be interpreted cautiously: lexical diversity and keyword counts can show that the topic is present, but they do not prove that a claim is correct, sourced, or usefully contested. The current run has no research capability or answer-package schema active yet, so it cannot satisfy the show's factual-quality bar.

### Remaining limitations

1. `--model qwen2.5:0.5b` intentionally overrides the four-shell model map; model diversity was not tested.
2. The runner still uses round-robin peer recipients, so this is a panel transcript but not yet an intentional Host-led speaking order.
3. No web research, source citations, Fact-Checker package, or Editor output was produced.
4. The question event is signed and quoted, but the broader queue/screener pipeline is not yet connected.

## Decision

**Result: seeded-question and Ollama five-round test passed.** The next milestone is not more raw rounds; it is wiring the question queue and research/answer-package path so the panel can turn this discussion into a sourced, reviewable segment.
