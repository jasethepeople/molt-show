use crate::events::{create_signed_event, Event};
use crate::identity::Identity;
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GenesisAgent {
    pub agent_id: String,
    pub public_key: String,
    pub genesis_event: String,
    pub lineage_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GenesisCeremony {
    pub protocol_version: String,
    pub attesters: Vec<String>,
    pub initial_agents: Vec<GenesisAgent>,
}

#[derive(Debug, Error)]
pub enum GenesisError {
    #[error("genesis event must have no parents")]
    HasParents,
    #[error("genesis event type mismatch")]
    WrongType,
    #[error("genesis event failed integrity: {0}")]
    InvalidEvent(String),
}

pub fn create_genesis(
    attester: &Identity,
    protocol_version: &str,
    agents: &[Identity],
    timestamp: &str,
) -> Event {
    // Agent genesis_event is intentionally filled after event construction by reference
    // to the ceremony event ID; the ceremony remains the authoritative bootstrap record.
    let payload_agents: Vec<_> = agents
        .iter()
        .map(|a| {
            json!({
                "agent_id": a.agent_id,
                "public_key": a.public_key_hex(),
                "lineage_id": format!("lineage_{}", a.agent_id),
            })
        })
        .collect();
    create_signed_event(
        attester,
        "GENESIS_CEREMONY",
        "MOLT/0.1",
        0,
        0,
        timestamp,
        vec![],
        json!({
            "protocol_version": protocol_version,
            "attesters": [attester.agent_id],
            "initial_agents": payload_agents
        }),
        vec![],
        vec![],
    )
}

pub fn validate_genesis(event: &Event) -> Result<(), GenesisError> {
    if event.event_type != "GENESIS_CEREMONY" {
        return Err(GenesisError::WrongType);
    }
    if !event.parents.is_empty() {
        return Err(GenesisError::HasParents);
    }
    event
        .verify_integrity()
        .map_err(|e| GenesisError::InvalidEvent(e.to_string()))
}
