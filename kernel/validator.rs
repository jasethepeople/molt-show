//! `external_deterministic_validator_v1`: a deterministic bridge-design oracle.
//!
//! Contract (see `docs/validator-design.md`): a pure function from a design
//! artifact to a verdict. Given the same artifact bytes it MUST return the same
//! verdict, byte-for-byte, on any machine — which is what makes a bridge-design
//! *outcome* reproducible even though agent *behavior* is not.
//!
//! The structural model is a deliberately small, fully deterministic 2D
//! pin-jointed truss solver: linear axial stiffness, small displacements, dense
//! Gaussian elimination with partial pivoting. It is a v1 oracle, not a general
//! FEM package. It enforces the six checks named in
//! `experiments/emergence-001/objective.json` under explicitly documented
//! assumptions, and reads no clock, no RNG state, and no agent identity.

use crate::hash::{blake3_hex, canonical_json_bytes};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

pub const VALIDATOR_VERSION: &str = "v1";

/// The six checks declared in `objective.json` → `validation.required`, in the
/// same order. A verdict carries an outcome for each.
pub const REQUIRED_CHECKS_V1: [&str; 6] = [
    "span_geometry_valid",
    "load_case_passes",
    "material_limits_pass",
    "deflection_limit_passes",
    "stability_checks_pass",
    "evidence_bundle_present",
];

/// Fixed per-check weights, declared with the validator version. They sum to
/// 1.0; `score` is derived from these and nothing else.
pub const CHECK_WEIGHTS_V1: [(&str, f64); 6] = [
    ("span_geometry_valid", 0.10),
    ("load_case_passes", 0.20),
    ("material_limits_pass", 0.25),
    ("deflection_limit_passes", 0.15),
    ("stability_checks_pass", 0.25),
    ("evidence_bundle_present", 0.05),
];

/// Relative tolerance on the declared span.
pub const SPAN_TOLERANCE_REL: f64 = 1e-3;
/// Serviceability limit: max nodal displacement ≤ span / this ratio.
pub const DEFAULT_DEFLECTION_LIMIT_RATIO: f64 = 250.0;
/// Absolute equilibrium residual (newtons) permitted on the free degrees of freedom.
pub const EQUILIBRIUM_TOLERANCE_N: f64 = 1e-3;
/// Tolerance on the utilization ratio (stress / allowable) and the deflection limit.
pub const UTILIZATION_TOLERANCE: f64 = 1e-9;
/// Tolerance on the deflection limit, in metres.
pub const DEFLECTION_TOLERANCE_M: f64 = 1e-9;
/// Below this magnitude a stiffness pivot is treated as singular.
pub const PIVOT_EPS: f64 = 1e-9;
/// Zero members shorter than this (metres) are degenerate.
pub const LENGTH_EPS: f64 = 1e-12;
/// Reported scalars are rounded to this many decimals so verdicts are stable.
pub const REPORT_DECIMALS: u32 = 6;

// ---------------------------------------------------------------------------
// Artifact
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Node {
    pub node_id: String,
    pub x_m: f64,
    pub y_m: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Member {
    pub member_id: String,
    pub node_a: String,
    pub node_b: String,
    pub material_id: String,
    pub section_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Material {
    pub material_id: String,
    pub allowable_stress_pa: f64,
    pub youngs_modulus_pa: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Section {
    pub section_id: String,
    pub area_m2: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Support {
    pub node_id: String,
    pub fix_x: bool,
    pub fix_y: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodalLoad {
    pub node_id: String,
    pub fx_n: f64,
    pub fy_n: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoadCase {
    pub load_case_id: String,
    pub loads: Vec<NodalLoad>,
}

/// A self-contained, canonically serializable bridge design.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DesignArtifact {
    pub design_id: String,
    pub span_m: f64,
    pub nodes: Vec<Node>,
    pub members: Vec<Member>,
    pub materials: Vec<Material>,
    pub sections: Vec<Section>,
    pub supports: Vec<Support>,
    pub load_cases: Vec<LoadCase>,
    pub evidence_refs: Vec<String>,
}

/// The set of evidence ids the caller can vouch for (e.g. `EVIDENCE`-kind events
/// recovered from the log). `evidence_bundle_present` is only true when the
/// artifact references at least one id and every id is in this set.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceBundle {
    pub known_refs: BTreeSet<String>,
}

impl EvidenceBundle {
    pub fn from_refs<I: IntoIterator<Item = String>>(refs: I) -> Self {
        Self {
            known_refs: refs.into_iter().collect(),
        }
    }

    /// A bundle that vouches for nothing — every evidence reference will fail.
    pub fn absent() -> Self {
        Self::default()
    }
}

// ---------------------------------------------------------------------------
// Verdict
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CheckOutcome {
    Pass,
    Fail,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum VerdictStatus {
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValidatorVerdict {
    pub validator_id: String,
    pub validator_version: String,
    pub status: VerdictStatus,
    /// BLAKE3 over the artifact's canonical bytes (raw received bytes if the
    /// artifact could not be parsed).
    pub artifact_hash: String,
    pub checks: BTreeMap<String, CheckOutcome>,
    pub score: f64,
    pub evidence_bundle_present: bool,
    pub reason: Option<String>,
}

impl ValidatorVerdict {
    pub fn rejected(
        validator_id: &str,
        validator_version: &str,
        artifact_hash: &str,
        reason: &str,
    ) -> Self {
        Self {
            validator_id: validator_id.to_string(),
            validator_version: validator_version.to_string(),
            status: VerdictStatus::Rejected,
            artifact_hash: artifact_hash.to_string(),
            checks: BTreeMap::new(),
            score: 0.0,
            evidence_bundle_present: false,
            reason: Some(reason.to_string()),
        }
    }

    pub fn passed(&self) -> bool {
        self.status == VerdictStatus::Accepted
    }

    pub fn failed_checks(&self) -> Vec<String> {
        self.checks
            .iter()
            .filter(|(_, outcome)| **outcome == CheckOutcome::Fail)
            .map(|(name, _)| name.clone())
            .collect()
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ValidatorError> {
        serde_json::to_value(self)
            .map(|value| canonical_json_bytes(&value))
            .map_err(|error| ValidatorError::Serialization(error.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Error, PartialEq)]
pub enum ValidatorError {
    #[error("artifact is malformed: {0}")]
    Malformed(String),
    #[error("artifact contains no nodes")]
    NoNodes,
    #[error("artifact contains no members")]
    NoMembers,
    #[error("artifact contains no supports")]
    NoSupports,
    #[error("artifact contains no load cases")]
    NoLoadCases,
    #[error("duplicate node id {0}")]
    DuplicateNode(String),
    #[error("duplicate member id {0}")]
    DuplicateMember(String),
    #[error("member {member_id} references unknown node {node_id}")]
    UnknownNode { member_id: String, node_id: String },
    #[error("member {0} connects a node to itself")]
    SelfMember(String),
    #[error("member {0} has zero length")]
    DegenerateMember(String),
    #[error("support references unknown node {0}")]
    UnknownSupportNode(String),
    #[error("load case {load_case_id} references unknown node {node_id}")]
    UnknownLoadNode { load_case_id: String, node_id: String },
    #[error("member {member_id} references unknown material {material_id}")]
    UnknownMaterial { member_id: String, material_id: String },
    #[error("member {member_id} references unknown section {section_id}")]
    UnknownSection { member_id: String, section_id: String },
    #[error("span is {got} m, expected {expected} m within {tolerance} m")]
    SpanMismatch {
        expected: f64,
        got: f64,
        tolerance: f64,
    },
    #[error("structure is unstable (stiffness matrix is singular)")]
    UnstableStructure,
    #[error("non-finite value in {0}")]
    NonFinite(String),
    #[error("{0}")]
    CheckFailed(String),
    #[error("artifact serialization failed: {0}")]
    Serialization(String),
}

/// Parses artifact bytes. A parse failure is a structured error the caller turns
/// into a rejected verdict — never a panic.
pub fn parse_design_artifact(bytes: &[u8]) -> Result<DesignArtifact, ValidatorError> {
    serde_json::from_slice(bytes).map_err(|error| ValidatorError::Malformed(error.to_string()))
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// A design validator: a deterministic function from an artifact to a verdict.
///
/// Implementations MUST NOT read wall-clock time, RNG state, environment, or
/// agent identity; the verdict is a pure function of `(artifact, evidence)`.
pub trait DesignValidator {
    fn validator_id(&self) -> &str;
    fn validator_version(&self) -> &str;
    fn required_checks(&self) -> &'static [&'static str];

    /// Evaluates every required check and returns the verdict. Malformed or
    /// structurally invalid artifacts yield a *rejected verdict with a reason*,
    /// never a panic and never an early abort that hides the other checks.
    fn validate(&self, artifact: &DesignArtifact, evidence: &EvidenceBundle)
        -> ValidatorVerdict;

    /// Byte-oriented entry point. Unparseable input becomes a rejected verdict
    /// whose `artifact_hash` is over the raw bytes received.
    fn validate_bytes(&self, bytes: &[u8], evidence: &EvidenceBundle) -> ValidatorVerdict {
        match parse_design_artifact(bytes) {
            Ok(artifact) => self.validate(&artifact, evidence),
            Err(error) => ValidatorVerdict::rejected(
                self.validator_id(),
                self.validator_version(),
                &blake3_hex(bytes),
                &error.to_string(),
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// Concrete validator
// ---------------------------------------------------------------------------

/// The v1 oracle named by `objective.json`.
#[derive(Debug, Clone)]
pub struct ExternalDeterministicValidatorV1 {
    /// The span the design must actually achieve, in metres.
    pub required_span_m: f64,
    /// Serviceability limit ratio: max displacement ≤ span / ratio.
    pub deflection_limit_ratio: f64,
}

impl Default for ExternalDeterministicValidatorV1 {
    fn default() -> Self {
        Self::new()
    }
}

impl ExternalDeterministicValidatorV1 {
    pub fn new() -> Self {
        Self {
            required_span_m: 100.0,
            deflection_limit_ratio: DEFAULT_DEFLECTION_LIMIT_RATIO,
        }
    }

    pub fn with_required_span(required_span_m: f64) -> Self {
        Self {
            required_span_m,
            ..Self::new()
        }
    }

    fn artifact_hash(artifact: &DesignArtifact) -> Result<String, ValidatorError> {
        serde_json::to_value(artifact)
            .map(|value| blake3_hex(&canonical_json_bytes(&value)))
            .map_err(|error| ValidatorError::Serialization(error.to_string()))
    }

    fn check_geometry(&self, artifact: &DesignArtifact) -> Result<(), ValidatorError> {
        if artifact.nodes.is_empty() {
            return Err(ValidatorError::NoNodes);
        }
        if artifact.members.is_empty() {
            return Err(ValidatorError::NoMembers);
        }
        if artifact.supports.is_empty() {
            return Err(ValidatorError::NoSupports);
        }
        if !artifact.span_m.is_finite() {
            return Err(ValidatorError::NonFinite("span_m".into()));
        }

        let mut node_ids: BTreeSet<&str> = BTreeSet::new();
        for node in &artifact.nodes {
            if !node.x_m.is_finite() || !node.y_m.is_finite() {
                return Err(ValidatorError::NonFinite(format!("node {}", node.node_id)));
            }
            if !node_ids.insert(node.node_id.as_str()) {
                return Err(ValidatorError::DuplicateNode(node.node_id.clone()));
            }
        }

        let mut member_ids: BTreeSet<&str> = BTreeSet::new();
        for member in &artifact.members {
            if !member_ids.insert(member.member_id.as_str()) {
                return Err(ValidatorError::DuplicateMember(member.member_id.clone()));
            }
            if member.node_a == member.node_b {
                return Err(ValidatorError::SelfMember(member.member_id.clone()));
            }
            for node_id in [&member.node_a, &member.node_b] {
                if !node_ids.contains(node_id.as_str()) {
                    return Err(ValidatorError::UnknownNode {
                        member_id: member.member_id.clone(),
                        node_id: node_id.clone(),
                    });
                }
            }
        }

        for support in &artifact.supports {
            if !node_ids.contains(support.node_id.as_str()) {
                return Err(ValidatorError::UnknownSupportNode(support.node_id.clone()));
            }
        }

        let min_x = artifact
            .nodes
            .iter()
            .map(|node| node.x_m)
            .fold(f64::INFINITY, f64::min);
        let max_x = artifact
            .nodes
            .iter()
            .map(|node| node.x_m)
            .fold(f64::NEG_INFINITY, f64::max);
        let span = max_x - min_x;
        let expected = self.required_span_m;
        let tolerance = SPAN_TOLERANCE_REL * expected.abs().max(f64::EPSILON);
        if (span - expected).abs() > tolerance {
            return Err(ValidatorError::SpanMismatch {
                expected,
                got: span,
                tolerance,
            });
        }
        Ok(())
    }

    fn check_evidence(&self, artifact: &DesignArtifact, evidence: &EvidenceBundle) -> bool {
        !artifact.evidence_refs.is_empty()
            && artifact
                .evidence_refs
                .iter()
                .all(|reference| {
                    !reference.trim().is_empty() && evidence.known_refs.contains(reference)
                })
    }

    fn prepare(&self, artifact: &DesignArtifact) -> Result<Prepared, ValidatorError> {
        if artifact.nodes.is_empty() {
            return Err(ValidatorError::NoNodes);
        }
        if artifact.members.is_empty() {
            return Err(ValidatorError::NoMembers);
        }
        if artifact.supports.is_empty() {
            return Err(ValidatorError::NoSupports);
        }

        // Nodes are indexed in sorted order so assembly is independent of the
        // order the artifact happened to list them in.
        let mut nodes: Vec<&Node> = artifact.nodes.iter().collect();
        nodes.sort_by(|a, b| a.node_id.cmp(&b.node_id));
        let mut node_index: BTreeMap<String, usize> = BTreeMap::new();
        for (index, node) in nodes.iter().enumerate() {
            if !node.x_m.is_finite() || !node.y_m.is_finite() {
                return Err(ValidatorError::NonFinite(format!("node {}", node.node_id)));
            }
            if node_index.insert(node.node_id.clone(), index).is_some() {
                return Err(ValidatorError::DuplicateNode(node.node_id.clone()));
            }
        }

        let mut materials: BTreeMap<&str, &Material> = BTreeMap::new();
        for material in &artifact.materials {
            materials.insert(material.material_id.as_str(), material);
        }
        let mut sections: BTreeMap<&str, &Section> = BTreeMap::new();
        for section in &artifact.sections {
            sections.insert(section.section_id.as_str(), section);
        }

        let mut members: Vec<&Member> = artifact.members.iter().collect();
        members.sort_by(|a, b| a.member_id.cmp(&b.member_id));

        let mut resolved = Vec::new();
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for member in members {
            if !seen.insert(member.member_id.as_str()) {
                return Err(ValidatorError::DuplicateMember(member.member_id.clone()));
            }
            if member.node_a == member.node_b {
                return Err(ValidatorError::SelfMember(member.member_id.clone()));
            }
            let a = *node_index
                .get(&member.node_a)
                .ok_or_else(|| ValidatorError::UnknownNode {
                    member_id: member.member_id.clone(),
                    node_id: member.node_a.clone(),
                })?;
            let b = *node_index
                .get(&member.node_b)
                .ok_or_else(|| ValidatorError::UnknownNode {
                    member_id: member.member_id.clone(),
                    node_id: member.node_b.clone(),
                })?;
            let (node_a, node_b) = (nodes[a], nodes[b]);
            let dx = node_b.x_m - node_a.x_m;
            let dy = node_b.y_m - node_a.y_m;
            let length = (dx * dx + dy * dy).sqrt();
            if !length.is_finite() || length <= LENGTH_EPS {
                return Err(ValidatorError::DegenerateMember(member.member_id.clone()));
            }
            let material = materials
                .get(member.material_id.as_str())
                .ok_or_else(|| ValidatorError::UnknownMaterial {
                    member_id: member.member_id.clone(),
                    material_id: member.material_id.clone(),
                })?;
            let section = sections
                .get(member.section_id.as_str())
                .ok_or_else(|| ValidatorError::UnknownSection {
                    member_id: member.member_id.clone(),
                    section_id: member.section_id.clone(),
                })?;
            if !material.allowable_stress_pa.is_finite()
                || !material.youngs_modulus_pa.is_finite()
                || !section.area_m2.is_finite()
            {
                return Err(ValidatorError::NonFinite(format!(
                    "material/section of member {}",
                    member.member_id
                )));
            }
            if section.area_m2 <= 0.0 {
                return Err(ValidatorError::Malformed(format!(
                    "section {} must have positive area",
                    section.section_id
                )));
            }
            resolved.push(ResolvedMember {
                member_id: member.member_id.clone(),
                a,
                b,
                length,
                ea: material.youngs_modulus_pa * section.area_m2,
                area: section.area_m2,
                allowable: material.allowable_stress_pa,
                cx: dx / length,
                cy: dy / length,
            });
        }

        let ndof = node_index.len() * 2;
        let mut supports: Vec<&Support> = artifact.supports.iter().collect();
        supports.sort_by(|a, b| a.node_id.cmp(&b.node_id));
        let mut constrained: BTreeSet<usize> = BTreeSet::new();
        for support in supports {
            let index = *node_index
                .get(&support.node_id)
                .ok_or_else(|| ValidatorError::UnknownSupportNode(support.node_id.clone()))?;
            if support.fix_x {
                constrained.insert(2 * index);
            }
            if support.fix_y {
                constrained.insert(2 * index + 1);
            }
        }
        let free: Vec<usize> = (0..ndof).filter(|dof| !constrained.contains(dof)).collect();

        Ok(Prepared {
            ndof,
            free,
            node_index,
            members: resolved,
        })
    }

    fn assemble_stiffness(&self, prepared: &Prepared) -> Vec<Vec<f64>> {
        let mut stiffness = vec![vec![0.0_f64; prepared.ndof]; prepared.ndof];
        for member in &prepared.members {
            let scale = member.ea / member.length;
            let (cx, cy) = (member.cx, member.cy);
            let (ai, aj) = (2 * member.a, 2 * member.a + 1);
            let (bi, bj) = (2 * member.b, 2 * member.b + 1);
            let terms = [
                (ai, ai, cx * cx),
                (ai, aj, cx * cy),
                (ai, bi, -cx * cx),
                (ai, bj, -cx * cy),
                (aj, ai, cx * cy),
                (aj, aj, cy * cy),
                (aj, bi, -cx * cy),
                (aj, bj, -cy * cy),
                (bi, ai, -cx * cx),
                (bi, aj, -cx * cy),
                (bi, bi, cx * cx),
                (bi, bj, cx * cy),
                (bj, ai, -cx * cy),
                (bj, aj, -cy * cy),
                (bj, bi, cx * cy),
                (bj, bj, cy * cy),
            ];
            for (row, column, value) in terms {
                stiffness[row][column] += scale * value;
            }
        }
        stiffness
    }

    fn load_vector(
        &self,
        prepared: &Prepared,
        load_case: &LoadCase,
    ) -> Result<Vec<f64>, ValidatorError> {
        let mut force = vec![0.0_f64; prepared.ndof];
        let mut loads: Vec<&NodalLoad> = load_case.loads.iter().collect();
        loads.sort_by(|a, b| a.node_id.cmp(&b.node_id));
        for load in loads {
            if !load.fx_n.is_finite() || !load.fy_n.is_finite() {
                return Err(ValidatorError::NonFinite(format!(
                    "load on node {}",
                    load.node_id
                )));
            }
            let index = *prepared.node_index.get(&load.node_id).ok_or_else(|| {
                ValidatorError::UnknownLoadNode {
                    load_case_id: load_case.load_case_id.clone(),
                    node_id: load.node_id.clone(),
                }
            })?;
            force[2 * index] += load.fx_n;
            force[2 * index + 1] += load.fy_n;
        }
        Ok(force)
    }

    fn is_stable(&self, prepared: &Prepared) -> Result<(), ValidatorError> {
        let stiffness = self.assemble_stiffness(prepared);
        let zero = vec![0.0_f64; prepared.ndof];
        let (reduced, _) = reduce(&stiffness, &zero, &prepared.free);
        let probe = vec![1.0_f64; prepared.free.len()];
        solve_linear(reduced, probe)
            .map(|_| ())
            .ok_or(ValidatorError::UnstableStructure)
    }

    fn solve_all(
        &self,
        prepared: &Prepared,
        artifact: &DesignArtifact,
    ) -> Result<Vec<CaseSolution>, ValidatorError> {
        if artifact.load_cases.is_empty() {
            return Err(ValidatorError::NoLoadCases);
        }
        let mut cases: Vec<&LoadCase> = artifact.load_cases.iter().collect();
        cases.sort_by(|a, b| a.load_case_id.cmp(&b.load_case_id));
        let mut solutions = Vec::new();
        for case in cases {
            solutions.push(self.solve_case(prepared, case)?);
        }
        Ok(solutions)
    }

    fn solve_case(
        &self,
        prepared: &Prepared,
        load_case: &LoadCase,
    ) -> Result<CaseSolution, ValidatorError> {
        let stiffness = self.assemble_stiffness(prepared);
        let force = self.load_vector(prepared, load_case)?;
        let (reduced, reduced_force) = reduce(&stiffness, &force, &prepared.free);
        let reduced_solution = solve_linear(reduced, reduced_force)
            .ok_or(ValidatorError::UnstableStructure)?;

        let mut displacement = vec![0.0_f64; prepared.ndof];
        for (offset, dof) in prepared.free.iter().enumerate() {
            displacement[*dof] = reduced_solution[offset];
        }
        if displacement.iter().any(|value| !value.is_finite()) {
            return Err(ValidatorError::NonFinite("displacements".into()));
        }

        let residual = residual_inf(&stiffness, &force, &displacement, &prepared.free);

        let mut max_deflection = 0.0_f64;
        for node in 0..(prepared.ndof / 2) {
            let (ux, uy) = (displacement[2 * node], displacement[2 * node + 1]);
            let magnitude = (ux * ux + uy * uy).sqrt();
            if magnitude > max_deflection {
                max_deflection = magnitude;
            }
        }

        let mut max_utilization = 0.0_f64;
        let mut governing_member_id = String::new();
        for member in &prepared.members {
            let axial = [
                displacement[2 * member.a],
                displacement[2 * member.a + 1],
            ];
            let other = [
                displacement[2 * member.b],
                displacement[2 * member.b + 1],
            ];
            let elongation = (other[0] - axial[0]) * member.cx + (other[1] - axial[1]) * member.cy;
            let axial_force = member.ea / member.length * elongation;
            let stress = axial_force.abs() / member.area;
            let utilization = if member.allowable > 0.0 {
                stress / member.allowable
            } else {
                f64::INFINITY
            };
            if utilization > max_utilization {
                max_utilization = utilization;
                governing_member_id = member.member_id.clone();
            }
        }

        Ok(CaseSolution {
            max_deflection_m: max_deflection,
            max_utilization,
            governing_member_id,
            residual_inf: residual,
        })
    }
}

impl DesignValidator for ExternalDeterministicValidatorV1 {
    fn validator_id(&self) -> &str {
        "external_deterministic_validator_v1"
    }

    fn validator_version(&self) -> &str {
        VALIDATOR_VERSION
    }

    fn required_checks(&self) -> &'static [&'static str] {
        &REQUIRED_CHECKS_V1
    }

    fn validate(
        &self,
        artifact: &DesignArtifact,
        evidence: &EvidenceBundle,
    ) -> ValidatorVerdict {
        let artifact_hash = match Self::artifact_hash(artifact) {
            Ok(hash) => hash,
            Err(error) => {
                return ValidatorVerdict::rejected(
                    self.validator_id(),
                    self.validator_version(),
                    &blake3_hex(&[]),
                    &error.to_string(),
                )
            }
        };

        let mut checks: BTreeMap<String, CheckOutcome> = BTreeMap::new();
        let mut reasons: Vec<String> = Vec::new();

        record(
            &mut checks,
            &mut reasons,
            "span_geometry_valid",
            self.check_geometry(artifact),
        );

        let evidence_ok = self.check_evidence(artifact, evidence);
        record_check(&mut checks, &mut reasons, "evidence_bundle_present", evidence_ok, || {
            ValidatorError::CheckFailed(
                "no verifiable evidence references (refs must be non-empty and known)".into(),
            )
        });

        match self.prepare(artifact) {
            Err(error) => {
                for name in [
                    "stability_checks_pass",
                    "load_case_passes",
                    "material_limits_pass",
                    "deflection_limit_passes",
                ] {
                    record(&mut checks, &mut reasons, name, Err(error.clone()));
                }
            }
            Ok(prepared) => {
                record(
                    &mut checks,
                    &mut reasons,
                    "stability_checks_pass",
                    self.is_stable(&prepared),
                );

                match self.solve_all(&prepared, artifact) {
                    Err(error) => {
                        for name in [
                            "load_case_passes",
                            "material_limits_pass",
                            "deflection_limit_passes",
                        ] {
                            record(&mut checks, &mut reasons, name, Err(error.clone()));
                        }
                    }
                    Ok(solutions) => {
                        let load_ok = solutions.iter().all(|solution| {
                            solution.residual_inf.is_finite()
                                && solution.residual_inf <= EQUILIBRIUM_TOLERANCE_N
                        });
                        record_check(&mut checks, &mut reasons, "load_case_passes", load_ok, || {
                            let worst = solutions
                                .iter()
                                .map(|solution| solution.residual_inf)
                                .fold(0.0_f64, f64::max);
                            ValidatorError::CheckFailed(format!(
                                "equilibrium residual {} N exceeds {} N",
                                round_to(worst, REPORT_DECIMALS),
                                EQUILIBRIUM_TOLERANCE_N
                            ))
                        });

                        let material_ok = solutions.iter().all(|solution| {
                            solution.max_utilization.is_finite()
                                && solution.max_utilization <= 1.0 + UTILIZATION_TOLERANCE
                        });
                        record_check(
                            &mut checks,
                            &mut reasons,
                            "material_limits_pass",
                            material_ok,
                            || {
                                let worst = solutions.iter().max_by(|a, b| {
                                    a.max_utilization
                                        .partial_cmp(&b.max_utilization)
                                        .unwrap_or(std::cmp::Ordering::Equal)
                                });
                                match worst {
                                    Some(solution) => ValidatorError::CheckFailed(format!(
                                        "member {} has utilization {} exceeding allowable",
                                        solution.governing_member_id,
                                        round_to(solution.max_utilization, REPORT_DECIMALS)
                                    )),
                                    None => ValidatorError::CheckFailed(
                                        "no load case produced a stress result".into(),
                                    ),
                                }
                            },
                        );

                        let limit = self.required_span_m / self.deflection_limit_ratio;
                        let deflection_ok = solutions.iter().all(|solution| {
                            solution.max_deflection_m.is_finite()
                                && solution.max_deflection_m <= limit + DEFLECTION_TOLERANCE_M
                        });
                        record_check(
                            &mut checks,
                            &mut reasons,
                            "deflection_limit_passes",
                            deflection_ok,
                            || {
                                let worst = solutions
                                    .iter()
                                    .map(|solution| solution.max_deflection_m)
                                    .fold(0.0_f64, f64::max);
                                ValidatorError::CheckFailed(format!(
                                    "peak deflection {} m exceeds limit {} m",
                                    round_to(worst, REPORT_DECIMALS),
                                    round_to(limit, REPORT_DECIMALS)
                                ))
                            },
                        );
                    }
                }
            }
        }

        let score = round_to(
            REQUIRED_CHECKS_V1
                .iter()
                .map(|name| {
                    let weight = check_weight(name);
                    if matches!(checks.get(*name), Some(CheckOutcome::Pass)) {
                        weight
                    } else {
                        0.0
                    }
                })
                .sum::<f64>(),
            REPORT_DECIMALS,
        );

        let accepted = REQUIRED_CHECKS_V1
            .iter()
            .all(|name| matches!(checks.get(*name), Some(CheckOutcome::Pass)));

        ValidatorVerdict {
            validator_id: self.validator_id().to_string(),
            validator_version: self.validator_version().to_string(),
            status: if accepted {
                VerdictStatus::Accepted
            } else {
                VerdictStatus::Rejected
            },
            artifact_hash,
            checks,
            score,
            evidence_bundle_present: evidence_ok,
            reason: if accepted {
                None
            } else {
                Some(reasons.join("; "))
            },
        }
    }
}

pub fn external_deterministic_validator_v1() -> ExternalDeterministicValidatorV1 {
    ExternalDeterministicValidatorV1::new()
}

pub fn check_weight(name: &str) -> f64 {
    CHECK_WEIGHTS_V1
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, weight)| *weight)
        .unwrap_or(0.0)
}

// ---------------------------------------------------------------------------
// Numeric kernel
// ---------------------------------------------------------------------------

struct ResolvedMember {
    member_id: String,
    a: usize,
    b: usize,
    length: f64,
    ea: f64,
    area: f64,
    allowable: f64,
    cx: f64,
    cy: f64,
}

struct Prepared {
    ndof: usize,
    free: Vec<usize>,
    node_index: BTreeMap<String, usize>,
    members: Vec<ResolvedMember>,
}

struct CaseSolution {
    max_deflection_m: f64,
    max_utilization: f64,
    /// The member carrying the highest utilization in this case — reported in
    /// the material-limit failure reason so a failing design is diagnosable.
    governing_member_id: String,
    residual_inf: f64,
}

fn reduce(
    stiffness: &[Vec<f64>],
    force: &[f64],
    free: &[usize],
) -> (Vec<Vec<f64>>, Vec<f64>) {
    let size = free.len();
    let mut reduced = vec![vec![0.0_f64; size]; size];
    let mut reduced_force = vec![0.0_f64; size];
    for (row, dof_row) in free.iter().enumerate() {
        reduced_force[row] = force[*dof_row];
        for (column, dof_column) in free.iter().enumerate() {
            reduced[row][column] = stiffness[*dof_row][*dof_column];
        }
    }
    (reduced, reduced_force)
}

/// Deterministic dense Gaussian elimination with partial pivoting. Returns
/// `None` when a pivot is below `PIVOT_EPS`, i.e. the reduced stiffness matrix
/// is singular (a mechanism) — the caller reports that as `UnstableStructure`.
#[allow(clippy::needless_range_loop)]
fn solve_linear(mut matrix: Vec<Vec<f64>>, mut rhs: Vec<f64>) -> Option<Vec<f64>> {
    let size = rhs.len();
    if size == 0 {
        return Some(Vec::new());
    }

    for column in 0..size {
        let mut pivot = column;
        for row in (column + 1)..size {
            if matrix[row][column].abs() > matrix[pivot][column].abs() {
                pivot = row;
            }
        }
        if matrix[pivot][column].abs() < PIVOT_EPS {
            return None;
        }
        if pivot != column {
            matrix.swap(column, pivot);
            rhs.swap(column, pivot);
        }
        let diagonal = matrix[column][column];
        let pivot_row = matrix[column].clone();
        let pivot_rhs = rhs[column];
        for row in (column + 1)..size {
            let factor = matrix[row][column] / diagonal;
            if factor == 0.0 {
                continue;
            }
            for (index, value) in pivot_row.iter().enumerate().skip(column) {
                matrix[row][index] -= factor * value;
            }
            rhs[row] -= factor * pivot_rhs;
        }
    }

    let mut solution = vec![0.0_f64; size];
    for row in (0..size).rev() {
        let tail: f64 = matrix[row][row + 1..]
            .iter()
            .zip(&solution[row + 1..])
            .map(|(coefficient, value)| coefficient * value)
            .sum();
        solution[row] = (rhs[row] - tail) / matrix[row][row];
    }
    Some(solution)
}

fn residual_inf(
    stiffness: &[Vec<f64>],
    force: &[f64],
    displacement: &[f64],
    free: &[usize],
) -> f64 {
    let mut worst = 0.0_f64;
    for dof in free {
        let row = &stiffness[*dof];
        let product: f64 = row
            .iter()
            .zip(displacement)
            .map(|(coefficient, value)| coefficient * value)
            .sum();
        let residual = (product - force[*dof]).abs();
        if residual > worst {
            worst = residual;
        }
    }
    worst
}

pub fn round_to(value: f64, decimals: u32) -> f64 {
    let factor = 10_f64.powi(decimals as i32);
    (value * factor).round() / factor
}

fn record(
    checks: &mut BTreeMap<String, CheckOutcome>,
    reasons: &mut Vec<String>,
    name: &str,
    result: Result<(), ValidatorError>,
) {
    match result {
        Ok(()) => {
            checks.insert(name.to_string(), CheckOutcome::Pass);
        }
        Err(error) => {
            checks.insert(name.to_string(), CheckOutcome::Fail);
            reasons.push(format!("{name}: {error}"));
        }
    }
}

fn record_check<F>(
    checks: &mut BTreeMap<String, CheckOutcome>,
    reasons: &mut Vec<String>,
    name: &str,
    passed: bool,
    on_failure: F,
) where
    F: FnOnce() -> ValidatorError,
{
    if passed {
        checks.insert(name.to_string(), CheckOutcome::Pass);
    } else {
        checks.insert(name.to_string(), CheckOutcome::Fail);
        reasons.push(format!("{name}: {}", on_failure()));
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn steel() -> Material {
        Material {
            material_id: "STEEL".into(),
            allowable_stress_pa: 250e6,
            youngs_modulus_pa: 200e9,
        }
    }

    fn section() -> Section {
        Section {
            section_id: "S1".into(),
            area_m2: 0.01,
        }
    }

    /// A stable triangular truss spanning exactly 100 m:
    ///
    /// ```text
    ///            N2 (50, 10)
    ///           /  \
    ///          /    \
    ///   N1 (0,0)----N3 (100,0)     load at N2, supports at N1 (x,y) and N3 (y)
    /// ```
    fn valid_artifact() -> DesignArtifact {
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
            materials: vec![steel()],
            sections: vec![section()],
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

    fn bundle() -> EvidenceBundle {
        EvidenceBundle::from_refs(["EV-1".to_string()])
    }

    fn validator() -> ExternalDeterministicValidatorV1 {
        ExternalDeterministicValidatorV1::new()
    }

    #[test]
    fn a_valid_design_is_accepted_with_full_score() {
        let verdict = validator().validate(&valid_artifact(), &bundle());
        assert!(verdict.passed(), "reason: {:?}", verdict.reason);
        assert_eq!(verdict.score, 1.0);
        assert!(verdict.failed_checks().is_empty());
        for name in REQUIRED_CHECKS_V1 {
            assert_eq!(verdict.checks.get(name), Some(&CheckOutcome::Pass), "{name}");
        }
    }

    #[test]
    fn weights_sum_to_one() {
        let total: f64 = CHECK_WEIGHTS_V1.iter().map(|(_, weight)| weight).sum();
        assert!((total - 1.0).abs() < 1e-12, "weights sum to {total}");
    }

    #[test]
    fn geometry_that_does_not_span_the_objective_fails() {
        let mut artifact = valid_artifact();
        artifact.nodes[2].x_m = 90.0; // only 90 m of span
        let verdict = validator().validate(&artifact, &bundle());
        assert_eq!(verdict.checks.get("span_geometry_valid"), Some(&CheckOutcome::Fail));
        assert_eq!(verdict.checks.get("stability_checks_pass"), Some(&CheckOutcome::Pass));
    }

    #[test]
    fn missing_evidence_fails_only_the_evidence_check() {
        let mut artifact = valid_artifact();
        artifact.evidence_refs.clear();
        let verdict = validator().validate(&artifact, &bundle());
        assert_eq!(verdict.checks.get("evidence_bundle_present"), Some(&CheckOutcome::Fail));
        assert!(!verdict.evidence_bundle_present);
        assert_eq!(verdict.score, round_to(1.0 - check_weight("evidence_bundle_present"), 6));
        assert_eq!(verdict.failed_checks(), vec!["evidence_bundle_present".to_string()]);
    }

    #[test]
    fn unknown_evidence_reference_fails() {
        let mut artifact = valid_artifact();
        artifact.evidence_refs.push("EV-UNKNOWN".into());
        let verdict = validator().validate(&artifact, &bundle());
        assert_eq!(verdict.checks.get("evidence_bundle_present"), Some(&CheckOutcome::Fail));
    }

    #[test]
    fn unknown_material_fails_dependent_checks_not_geometry() {
        let mut artifact = valid_artifact();
        artifact.members[0].material_id = "UNOBTAINIUM".into();
        let verdict = validator().validate(&artifact, &bundle());
        assert_eq!(verdict.checks.get("span_geometry_valid"), Some(&CheckOutcome::Pass));
        assert_eq!(verdict.checks.get("evidence_bundle_present"), Some(&CheckOutcome::Pass));
        for name in [
            "stability_checks_pass",
            "load_case_passes",
            "material_limits_pass",
            "deflection_limit_passes",
        ] {
            assert_eq!(verdict.checks.get(name), Some(&CheckOutcome::Fail), "{name}");
        }
    }

    #[test]
    fn a_mechanism_fails_stability() {
        let mut artifact = valid_artifact();
        // Keep only the bottom chord: N2 becomes an unbraced, unloaded dof.
        artifact.members.retain(|member| member.member_id == "M3");
        let verdict = validator().validate(&artifact, &bundle());
        assert_eq!(verdict.checks.get("stability_checks_pass"), Some(&CheckOutcome::Fail));
        assert!(!verdict.passed());
    }

    #[test]
    fn overstressed_members_fail_the_material_check() {
        let mut artifact = valid_artifact();
        artifact.materials[0].allowable_stress_pa = 1.0; // absurdly low
        let verdict = validator().validate(&artifact, &bundle());
        assert_eq!(verdict.checks.get("material_limits_pass"), Some(&CheckOutcome::Fail));
        assert_eq!(verdict.checks.get("stability_checks_pass"), Some(&CheckOutcome::Pass));
    }

    #[test]
    fn excessive_deflection_fails_the_serviceability_check() {
        let strict = ExternalDeterministicValidatorV1 {
            deflection_limit_ratio: 1e9, // limit ≈ 1e-7 m
            ..ExternalDeterministicValidatorV1::new()
        };
        let verdict = strict.validate(&valid_artifact(), &bundle());
        assert_eq!(verdict.checks.get("deflection_limit_passes"), Some(&CheckOutcome::Fail));
        assert_eq!(verdict.checks.get("material_limits_pass"), Some(&CheckOutcome::Pass));
    }

    #[test]
    fn malformed_bytes_are_rejected_not_panicked() {
        let verdict = validator().validate_bytes(b"{not json", &bundle());
        assert_eq!(verdict.status, VerdictStatus::Rejected);
        assert!(verdict.checks.is_empty());
        assert_eq!(verdict.score, 0.0);
        assert!(verdict.reason.is_some());
    }

    #[test]
    fn the_same_artifact_yields_an_identical_verdict() {
        let first = validator().validate(&valid_artifact(), &bundle());
        let second = validator().validate(&valid_artifact(), &bundle());
        assert_eq!(first, second);
        assert_eq!(first.canonical_bytes().unwrap(), second.canonical_bytes().unwrap());
    }

    #[test]
    fn analysis_is_independent_of_declaration_order() {
        let mut shuffled = valid_artifact();
        shuffled.nodes.reverse();
        shuffled.members.reverse();
        shuffled.supports.reverse();
        if let Some(case) = shuffled.load_cases.first_mut() {
            case.loads.reverse();
        }
        let original = validator().validate(&valid_artifact(), &bundle());
        let reordered = validator().validate(&shuffled, &bundle());
        assert_eq!(original.checks, reordered.checks);
        assert_eq!(original.score, reordered.score);
        assert_eq!(original.status, reordered.status);
    }

    #[test]
    fn validate_bytes_round_trips_a_json_artifact() {
        let bytes = serde_json::to_vec(&valid_artifact()).unwrap();
        let verdict = validator().validate_bytes(&bytes, &bundle());
        assert!(verdict.passed(), "reason: {:?}", verdict.reason);
    }
}
