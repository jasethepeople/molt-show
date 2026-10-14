//! Human review queue operations for gate results.
//!
//! List pending items:
//!   cargo run --bin review -- list --queue human-review.jsonl
//!
//! Append a decision without mutating the original queue:
//!   cargo run --bin review -- decide --queue human-review.jsonl \
//!     --decisions review-decisions.jsonl --target TARGET \
//!     --reviewer producer --decision revise --note "Add sources"

use molt_kernel::gate::{append_jsonl, ReviewDecision, ReviewQueueItem};
use std::env;
use std::fs;

fn option(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}

fn read_queue(path: &str) -> Result<Vec<ReviewQueueItem>, String> {
    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    raw.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).map_err(|e| e.to_string()))
        .collect()
}

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    let command = args.get(1).ok_or("review requires list or decide")?;
    let queue_path = option(&args, "--queue").ok_or("review requires --queue PATH")?;
    let queue = read_queue(&queue_path)?;

    match command.as_str() {
        "list" => {
            for item in queue {
                println!(
                    "{}\t{}\t{}\t{}",
                    item.queue_id, item.target_id, item.verdict, item.status
                );
            }
            Ok(())
        }
        "decide" => {
            let target_id = option(&args, "--target").ok_or("decide requires --target ID")?;
            let reviewer = option(&args, "--reviewer").ok_or("decide requires --reviewer ID")?;
            let decision = option(&args, "--decision").ok_or("decide requires --decision VALUE")?;
            if !matches!(decision.as_str(), "approve" | "reject" | "revise") {
                return Err("--decision must be approve, reject, or revise".into());
            }
            let item = queue
                .iter()
                .find(|item| item.target_id == target_id)
                .ok_or_else(|| format!("no queued target {target_id}"))?;
            let note = option(&args, "--note").unwrap_or_default();
            let decisions_path =
                option(&args, "--decisions").ok_or("decide requires --decisions PATH")?;
            let record = ReviewDecision {
                queue_id: item.queue_id.clone(),
                target_id,
                reviewer,
                decision: decision.clone(),
                note,
                status: "recorded".into(),
            };
            append_jsonl(&decisions_path, &record)?;
            println!("recorded {decision} for {}", record.target_id);
            Ok(())
        }
        other => Err(format!(
            "unknown review command {other}; expected list or decide"
        )),
    }
}
