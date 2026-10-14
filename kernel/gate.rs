//! Deterministic pre-publication checks for MOLT Show-001.
//!
//! The gate is intentionally outside agent decision-making. It consumes a
//! proposed script plus explicit source records and returns an observable
//! verdict. A future classifier can add findings, but it must not weaken these
//! deterministic checks.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceRecord {
    pub source_id: String,
    pub url: String,
    pub support: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GateRequest {
    pub target_id: String,
    pub script: String,
    #[serde(default)]
    pub sources: Vec<SourceRecord>,
    #[serde(default)]
    pub disclosure_present: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GateFinding {
    pub code: String,
    pub severity: String,
    pub message: String,
    pub excerpt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GateReport {
    pub target_id: String,
    pub verdict: String,
    pub findings: Vec<GateFinding>,
    pub checked_sentences: usize,
    pub cited_sentences: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewQueueItem {
    pub queue_id: String,
    pub target_id: String,
    pub verdict: String,
    pub status: String,
    pub report: GateReport,
}

pub fn review_queue_item(report: GateReport) -> ReviewQueueItem {
    ReviewQueueItem {
        queue_id: format!("gate-{}", report.target_id),
        target_id: report.target_id.clone(),
        verdict: report.verdict.clone(),
        status: "pending_human_review".to_string(),
        report,
    }
}

pub fn append_jsonl<T: Serialize>(path: &str, value: &T) -> Result<(), String> {
    let encoded = serde_json::to_string(value).map_err(|e| e.to_string())?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    writeln!(file, "{encoded}").map_err(|e| e.to_string())
}

pub fn evaluate(request: &GateRequest) -> GateReport {
    let sources: BTreeMap<&str, &SourceRecord> = request
        .sources
        .iter()
        .map(|source| (source.source_id.as_str(), source))
        .collect();
    let sentences = split_sentences(&request.script);
    let mut findings = Vec::new();
    let mut cited_sentences = 0;

    if !request.disclosure_present && !has_disclosure_language(&request.script) {
        findings.push(finding(
            "missing_ai_disclosure",
            "reject",
            "script must disclose that the show or content is AI-generated or AI-run",
            &request.script,
        ));
    }

    for sentence in &sentences {
        let citations = citations_in(sentence);
        if !citations.is_empty() {
            cited_sentences += 1;
        }
        for source_id in &citations {
            match sources.get(source_id.as_str()) {
                None => findings.push(finding(
                    "unknown_source_reference",
                    "reject",
                    &format!("citation [source:{source_id}] has no matching source record"),
                    sentence,
                )),
                Some(source) if !source.url.starts_with("https://") => findings.push(finding(
                    "invalid_source_url",
                    "reject",
                    "source records must use HTTPS URLs",
                    sentence,
                )),
                Some(source) if !support_matches(sentence, &source.support) => findings.push(
                    finding(
                        "source_does_not_support_claim",
                        "review",
                        &format!("source {source_id} is cited but its support text does not overlap the claim"),
                        sentence,
                    ),
                ),
                Some(_) => {}
            }
        }

        if is_quantified_or_named_claim(sentence) && citations.is_empty() {
            findings.push(finding(
                "unsourced_factual_claim",
                "reject",
                "quantified or named factual claim has no inline source citation",
                sentence,
            ));
        } else if looks_like_factual_claim(sentence) && citations.is_empty() {
            findings.push(finding(
                "unsupported_factual_language",
                "review",
                "factual-sounding language has no inline source citation",
                sentence,
            ));
        }
    }

    for (code, phrase) in [
        ("capability_leakage", "post_message"),
        ("capability_leakage", "web_research"),
        ("capability_leakage", "propose_segment"),
        ("capability_leakage", "raise_objection"),
        ("prompt_leakage", "system prompt"),
        ("prompt_leakage", "developer message"),
    ] {
        if request.script.to_ascii_lowercase().contains(phrase) {
            findings.push(finding(
                code,
                "reject",
                "internal capability or prompt terminology must not appear in audience-facing copy",
                phrase,
            ));
        }
    }

    if contains_phone_like_data(&request.script) {
        findings.push(finding(
            "personal_data",
            "reject",
            "possible phone number or private identifier in publishable text",
            "numeric identifier",
        ));
    }

    let verdict = if findings.iter().any(|f| f.severity == "reject") {
        "reject"
    } else if findings.iter().any(|f| f.severity == "review") {
        "review"
    } else {
        "pass"
    };

    GateReport {
        target_id: request.target_id.clone(),
        verdict: verdict.to_string(),
        findings,
        checked_sentences: sentences.len(),
        cited_sentences,
    }
}

fn finding(code: &str, severity: &str, message: &str, excerpt: &str) -> GateFinding {
    GateFinding {
        code: code.to_string(),
        severity: severity.to_string(),
        message: message.to_string(),
        excerpt: excerpt.trim().chars().take(240).collect(),
    }
}

fn split_sentences(text: &str) -> Vec<String> {
    text.split(|c| matches!(c, '.' | '!' | '?' | '\n'))
        .map(str::trim)
        .filter(|sentence| !sentence.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn has_disclosure_language(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [
        "ai-generated",
        "ai generated",
        "ai-run",
        "ai run",
        "artificial intelligence generated",
        "generated by artificial intelligence",
    ]
    .iter()
    .any(|phrase| lower.contains(phrase))
}

fn citations_in(sentence: &str) -> Vec<String> {
    let lower = sentence.to_ascii_lowercase();
    let mut result = Vec::new();
    let mut cursor = 0;
    while let Some(start) = lower[cursor..].find("[source:") {
        let absolute = cursor + start + "[source:".len();
        let Some(end) = lower[absolute..].find(']') else {
            break;
        };
        let id = sentence[absolute..absolute + end].trim();
        if !id.is_empty() {
            result.push(id.to_string());
        }
        cursor = absolute + end + 1;
        if cursor >= lower.len() {
            break;
        }
    }
    result
}

fn is_quantified_or_named_claim(sentence: &str) -> bool {
    if has_disclosure_language(sentence) {
        return false;
    }
    let has_digit = sentence.chars().any(|c| c.is_ascii_digit()) || sentence.contains('%');
    let words: Vec<&str> = sentence.split_whitespace().collect();
    let has_named_term = words.iter().skip(1).any(|word| {
        let cleaned = word.trim_matches(|c: char| !c.is_ascii_alphabetic());
        cleaned.len() > 2
            && cleaned
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_uppercase())
    });
    has_digit || has_named_term
}

fn looks_like_factual_claim(sentence: &str) -> bool {
    let lower = sentence.to_ascii_lowercase();
    [
        "according to",
        "research shows",
        "studies show",
        "reduces",
        "increases",
        "decreases",
        "is more",
        "is less",
        "costs",
        "causes",
        "leads to",
        "will improve",
    ]
    .iter()
    .any(|phrase| lower.contains(phrase))
}

fn support_matches(sentence: &str, support: &str) -> bool {
    let claim_tokens = meaningful_tokens(sentence);
    let support_tokens = meaningful_tokens(support);
    let overlap = claim_tokens
        .iter()
        .filter(|token| support_tokens.contains(token))
        .count();
    overlap >= 3 || (claim_tokens.len() <= 4 && overlap >= 2)
}

fn meaningful_tokens(text: &str) -> Vec<String> {
    text.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| token.len() >= 4)
        .map(ToOwned::to_owned)
        .collect()
}

fn contains_phone_like_data(text: &str) -> bool {
    let mut run = 0;
    for character in text.chars() {
        if character.is_ascii_digit() {
            run += 1;
            if run >= 7 {
                return true;
            }
        } else if !matches!(character, ' ' | '-' | '(' | ')' | '+' | '.') {
            run = 0;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(script: &str) -> GateRequest {
        GateRequest {
            target_id: "test-script".into(),
            script: script.into(),
            sources: vec![],
            disclosure_present: false,
        }
    }

    #[test]
    fn rejects_greenbrier_style_unsourced_stat() {
        let report = evaluate(&request(
            "The Greenbrier saw a 24% increase in ridership. This show is AI-run.",
        ));
        assert_eq!(report.verdict, "reject");
        assert!(report
            .findings
            .iter()
            .any(|finding| finding.code == "unsourced_factual_claim"));
    }

    #[test]
    fn accepts_a_cited_claim_with_matching_source() {
        let mut input = request(
            "The Greenbrier saw a 24% increase in ridership [source:S1]. This show is AI-run.",
        );
        input.sources.push(SourceRecord {
            source_id: "S1".into(),
            url: "https://example.com/greenbrier".into(),
            support: "Greenbrier saw a 24% increase in ridership".into(),
        });
        let report = evaluate(&input);
        assert_eq!(report.verdict, "pass");
    }

    #[test]
    fn rejects_missing_disclosure_and_internal_capability() {
        let report = evaluate(&request(
            "The panel used web_research and this is a factual answer.",
        ));
        assert_eq!(report.verdict, "reject");
        assert!(report
            .findings
            .iter()
            .any(|finding| finding.code == "missing_ai_disclosure"));
        assert!(report
            .findings
            .iter()
            .any(|finding| finding.code == "capability_leakage"));
    }

    #[test]
    fn returns_review_for_uncited_factual_sounding_language() {
        let report = evaluate(&request(
            "Public transit reduces congestion. This show is AI-run.",
        ));
        assert_eq!(report.verdict, "review");
        assert!(report
            .findings
            .iter()
            .any(|finding| finding.code == "unsupported_factual_language"));
    }
}
