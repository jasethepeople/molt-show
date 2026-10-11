use crate::events::Event;
use crate::memory::{MemoryKind, MemoryRecord, MemoryStore};
use crate::resources::{Lease, ResourcePool};
use crate::store::{EventStore, StoreError};
use crate::world::{replay, ReplayError, StateSnapshot, WorldState};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ObservatoryError {
    #[error("store error: {0}")]
    Store(#[from] StoreError),
    #[error("replay error: {0}")]
    Replay(#[from] ReplayError),
    #[error("event {0} was not found")]
    EventNotFound(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerificationReport {
    pub event_count: usize,
    pub valid: bool,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Lineage {
    pub root: String,
    pub ancestors: Vec<Event>,
    pub descendants: Vec<Event>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceReport {
    pub pools: Vec<ResourcePool>,
    pub leases: Vec<Lease>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProvenanceRecord {
    pub event_id: String,
    pub actor_id: String,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub evidence_refs: Vec<String>,
    pub resource_refs: Vec<String>,
}

pub struct Observatory<'a, S: EventStore, M: MemoryStore> {
    store: &'a S,
    memory: &'a M,
}

impl<'a, S: EventStore, M: MemoryStore> Observatory<'a, S, M> {
    pub fn new(store: &'a S, memory: &'a M) -> Self {
        Self { store, memory }
    }
    pub fn history(
        &self,
        agent: Option<&str>,
        event_type: Option<&str>,
    ) -> Result<Vec<Event>, ObservatoryError> {
        Ok(self
            .store
            .all()?
            .into_iter()
            .filter(|event| {
                agent.map(|a| event.actor_id == a).unwrap_or(true)
                    && event_type
                        .map(|kind| event.event_type == kind)
                        .unwrap_or(true)
            })
            .collect())
    }
    pub fn lineage(&self, event_id: &str) -> Result<Lineage, ObservatoryError> {
        let events = self.store.all()?;
        let root = events
            .iter()
            .find(|event| event.event_id == event_id)
            .cloned()
            .ok_or_else(|| ObservatoryError::EventNotFound(event_id.into()))?;
        let by_id: BTreeMap<_, _> = events
            .iter()
            .map(|event| (event.event_id.as_str(), event))
            .collect();
        let mut seen = BTreeSet::new();
        let mut queue = VecDeque::from([event_id.to_owned()]);
        let mut ancestors = Vec::new();
        while let Some(id) = queue.pop_front() {
            if let Some(event) = by_id.get(id.as_str()) {
                for parent in &event.parents {
                    if seen.insert(parent.clone()) {
                        if let Some(parent_event) = by_id.get(parent.as_str()) {
                            ancestors.push((*parent_event).clone());
                            queue.push_back(parent.clone());
                        }
                    }
                }
            }
        }
        let descendants = events
            .iter()
            .filter(|event| {
                let mut pending = event.parents.clone();
                let mut checked = BTreeSet::new();
                while let Some(parent) = pending.pop() {
                    if parent == event_id {
                        return true;
                    }
                    if checked.insert(parent.clone()) {
                        if let Some(parent_event) = by_id.get(parent.as_str()) {
                            pending.extend(parent_event.parents.iter().cloned());
                        }
                    }
                }
                false
            })
            .cloned()
            .collect();
        Ok(Lineage {
            root: root.event_id,
            ancestors,
            descendants,
        })
    }
    pub fn memory(
        &self,
        agent: &str,
        kind: Option<MemoryKind>,
    ) -> Result<Vec<MemoryRecord>, ObservatoryError> {
        self.memory
            .recall(agent, kind)
            .map_err(|error| ObservatoryError::EventNotFound(error.to_string()))
    }
    pub fn capabilities(&self, agent: Option<&str>) -> Result<Vec<Event>, ObservatoryError> {
        self.history(agent, Some("CAPABILITY_INVOKED"))
    }
    pub fn resources(&self, agent: Option<&str>) -> Result<ResourceReport, ObservatoryError> {
        let state = replay(&self.store.all()?)?;
        let leases = state
            .resources
            .leases
            .values()
            .filter(|lease| {
                agent
                    .map(|id| lease.grantee_id == id || lease.grantor_id == id)
                    .unwrap_or(true)
            })
            .cloned()
            .collect();
        Ok(ResourceReport {
            pools: state.resources.pools.values().cloned().collect(),
            leases,
        })
    }
    pub fn provenance(
        &self,
        agent: Option<&str>,
        event_type: Option<&str>,
    ) -> Result<Vec<ProvenanceRecord>, ObservatoryError> {
        Ok(self
            .history(agent, event_type)?
            .into_iter()
            .map(|event| ProvenanceRecord {
                event_id: event.event_id,
                actor_id: event.actor_id,
                event_type: event.event_type,
                payload: event.payload,
                evidence_refs: event.evidence_refs,
                resource_refs: event.resource_refs,
            })
            .collect())
    }
    pub fn claims(&self, agent: Option<&str>) -> Result<Vec<ProvenanceRecord>, ObservatoryError> {
        self.provenance(agent, Some("CLAIM_CREATED"))
    }
    pub fn evidence(&self, agent: Option<&str>) -> Result<Vec<ProvenanceRecord>, ObservatoryError> {
        self.provenance(agent, Some("EVIDENCE_SUBMITTED"))
    }
    pub fn challenges(
        &self,
        agent: Option<&str>,
    ) -> Result<Vec<ProvenanceRecord>, ObservatoryError> {
        self.provenance(agent, Some("CHALLENGE_RAISED"))
    }
    pub fn snapshot(&self) -> Result<StateSnapshot, ObservatoryError> {
        Ok(self.store.snapshot()?)
    }
    pub fn replay_at(&self, target: &str) -> Result<WorldState, ObservatoryError> {
        let events = self.store.all()?;
        let target_sequence = events
            .iter()
            .find(|event| event.event_id == target)
            .map(|event| event.sequence_number)
            .ok_or_else(|| ObservatoryError::EventNotFound(target.into()))?;
        Ok(replay(
            &events
                .into_iter()
                .filter(|candidate| candidate.sequence_number <= target_sequence)
                .collect::<Vec<_>>(),
        )?)
    }
    pub fn state_hash_at(&self, target: &str) -> Result<String, ObservatoryError> {
        Ok(self.replay_at(target)?.hash())
    }
    pub fn verify_history(&self) -> Result<VerificationReport, ObservatoryError> {
        let mut accepted = Vec::new();
        let mut errors = Vec::new();
        for event in self.store.all()? {
            if let Err(error) = crate::invariants::validate_history(&event, &accepted) {
                errors.push(format!("{}: {}", event.event_id, error));
            } else {
                accepted.push(event);
            }
        }
        Ok(VerificationReport {
            event_count: accepted.len() + errors.len(),
            valid: errors.is_empty(),
            errors,
        })
    }
}
