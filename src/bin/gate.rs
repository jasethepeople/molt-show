//! Evaluate a proposed show script before publication.
//!
//! Usage:
//!   cargo run --bin gate -- --input proposal.json --output verdict.json
//!
//! The input is a `molt_kernel::gate::GateRequest`; the output is a
//! `molt_kernel::gate::GateReport`. The gate never calls an agent or edits the
//! proposed script.

use molt_kernel::gate::{evaluate, GateRequest};
use std::env;
use std::fs;

fn option(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    let input_path = option(&args, "--input").ok_or("gate requires --input PATH")?;
    let output_path = option(&args, "--output");
    let raw = fs::read_to_string(&input_path).map_err(|e| e.to_string())?;
    let request: GateRequest = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let report = evaluate(&request);
    let rendered = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;

    if let Some(path) = output_path {
        fs::write(&path, format!("{rendered}\n")).map_err(|e| e.to_string())?;
        eprintln!("wrote gate report to {path}");
    } else {
        println!("{rendered}");
    }

    if report.verdict == "reject" {
        return Err("gate verdict: reject".into());
    }
    Ok(())
}
