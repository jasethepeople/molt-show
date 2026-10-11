use crate::events::Event;
use crate::hash::{blake3_hex, canonical_json_bytes};
use crate::resources::{ResourceError, ResourceState};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct WorldState {
    pub resources: ResourceState,
    pub event_count: u64,
    pub last_event_id: Option<String>,
    pub event_types: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StateSnapshot {
    pub state: WorldState,
    pub state_hash: String,
}

#[derive(Debug, Error)]
pub enum ReplayError {
    #[error("history is not ordered by contiguous sequence: expected {expected}, got {got}")]
    Sequence { expected: u64, got: u64 },
    #[error("event {event_id} failed integrity: {reason}")]
    InvalidEvent { event_id: String, reason: String },
    #[error("event {event_id} failed resource transition: {source}")]
    Resource {
        event_id: String,
        source: ResourceError,
    },
    #[error("state serialization failed: {0}")]
    Serialization(String),
}

impl WorldState {
    pub fn apply(&mut self, event: &Event) -> Result<(), ReplayError> {
        self.resources
            .apply_event(&event.event_type, &event.payload, &event.actor_id)
            .map_err(|source| ReplayError::Resource {
                event_id: event.event_id.clone(),
                source,
            })?;
        *self
            .event_types
            .entry(event.event_type.clone())
            .or_default() += 1;
        self.event_count += 1;
        self.last_event_id = Some(event.event_id.clone());
        Ok(())
    }

    pub fn hash(&self) -> String {
        blake3_hex(&canonical_json_bytes(
            &serde_json::to_value(self).expect("WorldState is serializable"),
        ))
    }

    pub fn snapshot(&self) -> StateSnapshot {
        StateSnapshot {
            state: self.clone(),
            state_hash: self.hash(),
        }
    }
}

pub fn replay(events: &[Event]) -> Result<WorldState, ReplayError> {
    let mut ordered = events.to_vec();
    ordered.sort_by_key(|event| event.sequence_number);
    let mut state = WorldState::default();
    let mut expected = ordered
        .first()
        .map(|event| event.sequence_number)
        .unwrap_or(0);
    for event in &ordered {
        if event.sequence_number != expected {
            return Err(ReplayError::Sequence {
                expected,
                got: event.sequence_number,
            });
        }
        event
            .verify_integrity()
            .map_err(|error| ReplayError::InvalidEvent {
                event_id: event.event_id.clone(),
                reason: error.to_string(),
            })?;
        state.apply(event)?;
        expected = expected.saturating_add(1);
    }
    Ok(state)
}

pub fn hash_state(state: &WorldState) -> String {
    state.hash()
}
