use molt_kernel::agent::Message;
use molt_kernel::memory::{FileMemoryStore, MemoryKind};
use molt_kernel::observatory::Observatory;
use molt_kernel::store::FileEventStore;
use serde::Serialize;
use std::env;

fn option(args: &[String], name: &str, default: &str) -> String {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
        .unwrap_or_else(|| default.into())
}
fn agent(args: &[String]) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == "--agent")
        .map(|pair| pair[1].clone())
}
fn print_json<T: Serialize>(value: T) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?
    );
    Ok(())
}
fn value<T: Serialize, E: std::fmt::Display>(
    result: Result<T, E>,
) -> Result<serde_json::Value, String> {
    result
        .map_err(|e| e.to_string())
        .and_then(|v| serde_json::to_value(v).map_err(|e| e.to_string()))
}
fn usage() {
    eprintln!("usage: molt <history|events|lineage|memory|resources|claims|evidence|challenges|replay|state-hash|verify-history|chronicle> [options]\noptions: --store PATH --memory PATH --agent ID --type EVENT_TYPE --at EVENT_ID --kind episodic|semantic|evidence|relationship|lineage --out PATH");
}

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    let command = args.get(1).ok_or_else(|| {
        usage();
        "missing command".to_owned()
    })?;
    let store_path = option(&args, "--store", "events.jsonl");
    let memory_path = option(&args, "--memory", "memory.jsonl");
    let store = FileEventStore::open(&store_path).map_err(|e| e.to_string())?;
    let memory = FileMemoryStore::open(&memory_path).map_err(|e| e.to_string())?;
    let observatory = Observatory::new(&store, &memory);
    let selected_agent = agent(&args);

    if command == "chronicle" {
        // The raw, unedited transcript — every MESSAGE_SENT event in event
        // order, rendered as plain text instead of JSON. This is the thing
        // meant to feed a RAG index or a storytelling pass later: nothing
        // here is summarized or reordered, it's exactly what the log holds.
        let events = observatory
            .history(None, Some("MESSAGE_SENT"))
            .map_err(|e| e.to_string())?;
        let mut out = String::new();
        for event in &events {
            let Ok(message) = serde_json::from_value::<Message>(event.payload.clone()) else {
                continue;
            };
            let text = message.body.get("text").and_then(|t| t.as_str()).unwrap_or("");
            out.push_str(&format!(
                "[{}] {} -> {}: {}\n",
                event.timestamp, message.sender_id, message.recipient_id, text
            ));
        }
        let out_path = option(&args, "--out", "");
        if out_path.is_empty() {
            print!("{out}");
        } else {
            std::fs::write(&out_path, &out).map_err(|e| e.to_string())?;
            eprintln!("wrote {} messages to {out_path}", events.len());
        }
        return Ok(());
    }

    let output = match command.as_str() {
        "history" => value(observatory.history(selected_agent.as_deref(), None))?,
        "events" => value(
            observatory.history(
                selected_agent.as_deref(),
                args.windows(2)
                    .find(|p| p[0] == "--type")
                    .map(|p| p[1].as_str()),
            ),
        )?,
        "lineage" => value(observatory.lineage(args.get(2).ok_or("lineage requires EVENT_ID")?))?,
        "memory" => {
            let kind =
                args.windows(2)
                    .find(|p| p[0] == "--kind")
                    .and_then(|p| match p[1].as_str() {
                        "episodic" => Some(MemoryKind::Episodic),
                        "semantic" => Some(MemoryKind::Semantic),
                        "evidence" => Some(MemoryKind::Evidence),
                        "relationship" => Some(MemoryKind::Relationship),
                        "lineage" => Some(MemoryKind::Lineage),
                        _ => None,
                    });
            value(observatory.memory(&selected_agent.ok_or("memory requires --agent ID")?, kind))?
        }
        "resources" => value(observatory.resources(selected_agent.as_deref()))?,
        "claims" => value(observatory.claims(selected_agent.as_deref()))?,
        "evidence" => value(observatory.evidence(selected_agent.as_deref()))?,
        "challenges" => value(observatory.challenges(selected_agent.as_deref()))?,
        "replay" => value(observatory.replay_at(args.get(2).ok_or("replay requires EVENT_ID")?))?,
        "state-hash" => {
            value(observatory.state_hash_at(args.get(2).ok_or("state-hash requires EVENT_ID")?))?
        }
        "verify-history" => value(observatory.verify_history())?,
        _ => {
            usage();
            return Err(format!("unknown command: {command}"));
        }
    };
    print_json(output)
}
