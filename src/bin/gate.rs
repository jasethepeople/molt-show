//! Evaluate a proposed show script before publication.
//!
//! Usage:
//!   cargo run --bin gate -- --input proposal.json --output verdict.json \
//!     --audit gate-audit.jsonl --review-queue human-review.jsonl
//!
//! The input is a `molt_kernel::gate::GateRequest`; the output is a
//! `molt_kernel::gate::GateReport`. The gate never calls an agent or edits the
//! proposed script. `review` and `reject` both block publication and, when
//! configured, append an auditable item to the human-review queue.

use molt_kernel::gate::{append_jsonl, evaluate, review_queue_item, GateReport, GateRequest};
use std::env;
use std::fs;

fn option(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}

fn write_report(path: &str, report: &GateReport) -> Result<(), String> {
    let rendered = serde_json::to_string_pretty(report).map_err(|e| e.to_string())?;
    fs::write(path, format!("{rendered}\n")).map_err(|e| e.to_string())
}

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    let input_path = option(&args, "--input").ok_or("gate requires --input PATH")?;
    let output_path = option(&args, "--output");
    let audit_path = option(&args, "--audit");
    let review_queue_path = option(&args, "--review-queue");
    let raw = fs::read_to_string(&input_path).map_err(|e| e.to_string())?;
    let request: GateRequest = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let report = evaluate(&request);
    let rendered = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;

    if let Some(path) = audit_path {
        append_jsonl(&path, &report)?;
        eprintln!("appended gate audit to {path}");
    }

    if report.verdict != "pass" {
        if let Some(path) = review_queue_path {
            append_jsonl(&path, &review_queue_item(report.clone()))?;
            eprintln!("queued {} for human review at {path}", report.target_id);
        }
    }

    if let Some(path) = output_path {
        write_report(&path, &report)?;
        eprintln!("wrote gate report to {path}");
    } else {
        println!("{rendered}");
    }

    if report.verdict != "pass" {
        return Err(format!("gate verdict: {}", report.verdict));
    }
    Ok(())
}
