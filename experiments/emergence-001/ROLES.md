# Role slots (EMERGENCE-001)

`converse` reads an optional role file — `--roles`, default
`experiments/emergence-001/roles.json` — keyed by `agent_id`. Each agent's
role text is appended verbatim to that agent's persona seed. This is the one
knob for deciding what role a given agent plays. Omit the file, the agent, or
a field and that agent runs on its unmodified baseline seed.

## Fill this in

`roles.json` ships with one slot, for `A01` (agent 1 — the "Prophet" slot).
Replace the bracketed placeholders:

| Field           | Meaning                                                        |
|-----------------|----------------------------------------------------------------|
| `role`          | One line naming the role, e.g. `[AGENT 1 ROLE]`.               |
| `role_goals`    | List of goals the role pursues.                                |
| `role_behavior` | How the role should behave toward the other agents.            |

Add a slot for any other `agent_id` the same way; leave the rest out to keep
them baseline.

## Run

```bash
ANTHROPIC_API_KEY=... cargo run --bin converse -- --rounds 6 \
  --roles experiments/emergence-001/roles.json
```

Role text is seed text only: it never enters genesis, identity, or any signed
event. Identities and the store are untouched by it, so a resumed run keeps
the same agents. Changing a role changes only future behavior, generated
fresh from the seed.
