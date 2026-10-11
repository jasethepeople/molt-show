//! `cargo run --bin verify_freeze -- experiments/emergence-001/freeze.json`
//!
//! Recomputes BLAKE3 over each file freeze.json says it locked and reports
//! any mismatch. A baseline experiment is only meaningful if its config
//! genuinely hasn't moved since it was frozen — this is how you'd catch it
//! if it had.

use std::env;
use std::fs;
use std::path::Path;

fn main() -> Result<(), String> {
    let freeze_path = env::args()
        .nth(1)
        .unwrap_or_else(|| "experiments/emergence-001/freeze.json".to_string());
    let base = Path::new(&freeze_path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_default();

    let raw = fs::read_to_string(&freeze_path).map_err(|e| e.to_string())?;
    let freeze: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let frozen_files = freeze["frozen_files"]
        .as_object()
        .ok_or("freeze.json has no frozen_files object")?;

    let mut mismatches = Vec::new();
    for (name, expected_hash) in frozen_files {
        let expected = expected_hash.as_str().unwrap_or("");
        let file_path = base.join(name);
        let bytes = match fs::read(&file_path) {
            Ok(b) => b,
            Err(e) => {
                mismatches.push(format!("{name}: could not read ({e})"));
                continue;
            }
        };
        let actual = blake3::hash(&bytes).to_hex().to_string();
        if actual != expected {
            mismatches.push(format!("{name}: expected {expected}, got {actual}"));
        } else {
            println!("{name}: OK");
        }
    }

    if mismatches.is_empty() {
        println!("freeze intact: {} file(s) verified", frozen_files.len());
        Ok(())
    } else {
        for m in &mismatches {
            eprintln!("MISMATCH: {m}");
        }
        Err(format!("{} file(s) no longer match the freeze", mismatches.len()))
    }
}
