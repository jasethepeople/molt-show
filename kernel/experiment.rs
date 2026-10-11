use serde::{Deserialize, Serialize};

pub type ExperimentId = String;
pub type StateHash = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuditLevel {
    Minimal,
    Actions,
    ActionsAndEvidence,
    FullObservableTrace,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum InterventionPolicy {
    HumanControlled,
    HumanPauseOnly,
    EmergencyStopOnly,
    Autonomous,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PrivacyScope {
    Public,
    Participant,
    Institution,
    Private,
    Sealed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceScope {
    pub pool_ids: Vec<String>,
    pub maximum: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityScope {
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Objective {
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TerminationCondition {
    EventCount(u64),
    Explicit,
    Timestamp(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExperimentContext {
    pub experiment_id: ExperimentId,
    pub objective: Objective,
    pub participants: Vec<String>,
    pub initial_state: StateHash,
    pub resource_scope: ResourceScope,
    pub capability_scope: CapabilityScope,
    pub audit_policy: AuditLevel,
    pub intervention_policy: InterventionPolicy,
    pub privacy_scope: PrivacyScope,
    pub termination: TerminationCondition,
    pub genesis_event: String,
}

impl ExperimentContext {
    pub fn canonical_bytes(&self) -> Vec<u8> {
        crate::hash::canonical_json_bytes(
            &serde_json::to_value(self).expect("ExperimentContext is serializable"),
        )
    }
    pub fn hash(&self) -> String {
        crate::hash::blake3_hex(&self.canonical_bytes())
    }
}
