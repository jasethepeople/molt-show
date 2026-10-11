//! Integration coverage for `external_deterministic_validator_v1` through the
//! public API only, so the trait surface a real caller would use is exercised.

use molt_kernel::validator::{
    external_deterministic_validator_v1, DesignArtifact, DesignValidator, EvidenceBundle, LoadCase,
    Material, Member, NodalLoad, Node, Section, Support, VerdictStatus,
};

fn artifact() -> DesignArtifact {
    DesignArtifact {
        design_id: "bridge-100m".into(),
        span_m: 100.0,
        nodes: vec![
            Node { node_id: "N1".into(), x_m: 0.0, y_m: 0.0 },
            Node { node_id: "N2".into(), x_m: 50.0, y_m: 10.0 },
            Node { node_id: "N3".into(), x_m: 100.0, y_m: 0.0 },
        ],
        members: vec![
            Member { member_id: "M1".into(), node_a: "N1".into(), node_b: "N2".into(), material_id: "STEEL".into(), section_id: "S1".into() },
            Member { member_id: "M2".into(), node_a: "N2".into(), node_b: "N3".into(), material_id: "STEEL".into(), section_id: "S1".into() },
            Member { member_id: "M3".into(), node_a: "N1".into(), node_b: "N3".into(), material_id: "STEEL".into(), section_id: "S1".into() },
        ],
        materials: vec![Material {
            material_id: "STEEL".into(),
            allowable_stress_pa: 250e6,
            youngs_modulus_pa: 200e9,
        }],
        sections: vec![Section { section_id: "S1".into(), area_m2: 0.01 }],
        supports: vec![
            Support { node_id: "N1".into(), fix_x: true, fix_y: true },
            Support { node_id: "N3".into(), fix_x: false, fix_y: true },
        ],
        load_cases: vec![LoadCase {
            load_case_id: "LC1".into(),
            loads: vec![NodalLoad { node_id: "N2".into(), fx_n: 0.0, fy_n: -100_000.0 }],
        }],
        evidence_refs: vec!["EV-1".into()],
    }
}

#[test]
fn accepts_a_valid_design_and_reports_its_artifact_hash() {
    let validator = external_deterministic_validator_v1();
    let evidence = EvidenceBundle::from_refs(["EV-1".to_string()]);
    let verdict = validator.validate(&artifact(), &evidence);
    assert!(verdict.passed(), "reason: {:?}", verdict.reason);
    assert_eq!(verdict.validator_id, "external_deterministic_validator_v1");
    assert_eq!(verdict.validator_version, "v1");
    assert_eq!(verdict.artifact_hash.len(), 64); // BLAKE3 hex
    assert_eq!(verdict.required_checks_len(&validator), 6);
}

#[test]
fn rejects_malformed_bytes_without_panicking() {
    let validator = external_deterministic_validator_v1();
    let verdict = validator.validate_bytes(b"}{", &EvidenceBundle::absent());
    assert_eq!(verdict.status, VerdictStatus::Rejected);
    assert!(verdict.reason.is_some());
}

#[test]
fn is_deterministic_across_calls() {
    let validator = external_deterministic_validator_v1();
    let evidence = EvidenceBundle::from_refs(["EV-1".to_string()]);
    let first = validator.validate(&artifact(), &evidence);
    let second = validator.validate(&artifact(), &evidence);
    assert_eq!(first, second);
}

/// Small helper mirroring how a caller would confirm the validator advertises
/// the checks `objective.json` requires.
trait RequiredChecksLen {
    fn required_checks_len(&self, validator: &dyn DesignValidator) -> usize;
}

impl RequiredChecksLen for molt_kernel::validator::ValidatorVerdict {
    fn required_checks_len(&self, validator: &dyn DesignValidator) -> usize {
        validator.required_checks().len()
    }
}
