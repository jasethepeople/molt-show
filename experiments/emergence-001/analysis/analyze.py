#!/usr/bin/env python3
"""Deterministic, read-only EMERGENCE-001 history analysis."""
import argparse, hashlib, json
from collections import Counter, defaultdict
from pathlib import Path

CONFIG_FILES = ["genesis.json", "objective.json", "population.json", "capabilities.json", "resources.json", "policies.json"]

def load_jsonl(path):
    if not path:
        return []
    rows = []
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            if line.strip(): rows.append(json.loads(line))
    return rows

def config_hash(config_dir):
    digest = hashlib.blake2b(digest_size=32)
    for name in CONFIG_FILES:
        data = json.loads((config_dir / name).read_text(encoding="utf-8"))
        digest.update(json.dumps(data, sort_keys=True, separators=(",", ":")).encode())
    return digest.hexdigest()

def analyze(config_dir, events, memory, run_id):
    population = json.loads((config_dir / "population.json").read_text(encoding="utf-8"))
    prescribed_roles = set()
    agents = {row["agent_id"] for row in population["agents"]}
    message_edges = Counter()
    outgoing = Counter(); event_types = Counter(); per_agent = Counter()
    claim_events = []; evidence_events = []; challenge_events = []
    capability_events = []; resource_events = []
    for event in events:
        actor = event.get("actor_id", "")
        kind = event.get("event_type", "")
        event_types[kind] += 1; per_agent[actor] += 1
        payload = event.get("payload") or {}
        if kind == "MESSAGE_SENT":
            recipient = payload.get("recipient_id")
            if recipient and recipient != actor:
                message_edges[(actor, recipient)] += 1; outgoing[actor] += 1
        if kind == "CLAIM_CREATED": claim_events.append(event)
        if kind == "EVIDENCE_SUBMITTED": evidence_events.append(event)
        if kind == "CHALLENGE_RAISED": challenge_events.append(event)
        if kind == "CAPABILITY_INVOKED": capability_events.append(event)
        if kind.startswith("RESOURCE_"): resource_events.append(event)
    undirected = {tuple(sorted(edge)) for edge in message_edges}
    reciprocated = sum(1 for a, b in undirected if message_edges[(a, b)] and message_edges[(b, a)])
    observed_agents = sorted(agents | {a for edge in message_edges for a in edge})
    n = len(observed_agents)
    possible = n * (n - 1)
    degrees = Counter()
    for (sender, recipient), count in message_edges.items(): degrees[sender] += count; degrees[recipient] += count
    max_degree = max(degrees.values(), default=0)
    mean_degree = sum(degrees.values()) / n if n else 0
    centralization = ((max_degree * n) - sum(degrees.values())) / ((n - 2) * (n - 1)) if n > 2 else 0
    memory_kinds = Counter(row.get("kind") for row in memory)
    emergent_signals = []
    if len(undirected) > 0: emergent_signals.append("interaction_network")
    if any(message_edges[(a, b)] and message_edges[(b, a)] for a, b in undirected): emergent_signals.append("reciprocal_communication")
    if resource_events and any(event.get("event_type") == "RESOURCE_LEASE_ISSUED" and event.get("payload", {}).get("parent_lease_id") for event in resource_events): emergent_signals.append("delegated_resource_flow")
    return {
        "schema": "MOLT/EMERGENCE-001/analysis/0.1",
        "experiment_id": "EMERGENCE-001",
        "run_id": run_id,
        "configuration_hash": config_hash(config_dir),
        "input_counts": {"events": len(events), "memory_records": len(memory)},
        "interaction": {
            "messages_total": sum(message_edges.values()),
            "messages_per_agent": dict(sorted(outgoing.items())),
            "unique_contacts": len(undirected),
            "reciprocal_contacts": reciprocated,
            "network_density_directed": (sum(1 for edge in message_edges if message_edges[edge]) / possible) if possible else 0,
            "network_centralization": round(centralization, 12)
        },
        "information": {
            "claims": len(claim_events), "evidence": len(evidence_events), "challenges": len(challenge_events),
            "capability_invocations": len(capability_events), "memory_by_kind": dict(sorted(memory_kinds.items())),
            "event_types": dict(sorted(event_types.items()))
        },
        "organization_observations": {
            "delegated_leases": sum(1 for event in resource_events if event.get("event_type") == "RESOURCE_LEASE_ISSUED" and event.get("payload", {}).get("parent_lease_id")),
            "observed_agents": observed_agents,
            "emergent_signals": emergent_signals,
            "prescribed_roles": sorted(prescribed_roles),
            "novelty_note": "Signals are measurements, not objectives or proof of organization."
        },
        "resource_archaeology": {"resource_events": len(resource_events), "resource_event_types": dict(sorted(Counter(e.get("event_type") for e in resource_events).items()))},
        "reproducibility": {"agent_count_configured": len(agents), "event_order": "input_order_preserved", "analysis_deterministic": True}
    }

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--config-dir", type=Path, default=Path(__file__).parents[1])
    parser.add_argument("--events", type=Path, required=True)
    parser.add_argument("--memory", type=Path)
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    result = analyze(args.config_dir, load_jsonl(args.events), load_jsonl(args.memory) if args.memory else [], args.run_id)
    encoded = json.dumps(result, indent=2, sort_keys=True) + "\n"
    if args.output: args.output.write_text(encoded, encoding="utf-8")
    else: print(encoded, end="")

if __name__ == "__main__": main()
