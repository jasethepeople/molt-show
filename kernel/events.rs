use crate::hash::{blake3_hex, canonical_json_bytes};
use crate::identity::{verify, IdentityError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Event {
    pub event_id: String,
    pub event_type: String,
    pub protocol: String,
    pub actor_id: String,
    pub logical_clock: u64,
    pub sequence_number: u64,
    pub timestamp: String,
    pub parents: Vec<String>,
    pub payload: Value,
    pub evidence_refs: Vec<String>,
    pub resource_refs: Vec<String>,
    pub signature: String,
    pub content_hash: String,
}

#[derive(Debug, Error)]
pub enum EventError {
    #[error("event hash mismatch")]
    HashMismatch,
    #[error("event signature invalid: {0}")]
    Signature(#[from] IdentityError),
    #[error("event ID does not equal content hash")]
    EventIdMismatch,
}

impl Event {
    /// Deterministic bytes signed and hashed. Signature and content hash are excluded.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let unsigned = json!({
            "actor_id": self.actor_id,
            "event_type": self.event_type,
            "evidence_refs": self.evidence_refs,
            "logical_clock": self.logical_clock,
            "parents": self.parents,
            "payload": self.payload,
            "protocol": self.protocol,
            "resource_refs": self.resource_refs,
            "sequence_number": self.sequence_number,
            "timestamp": self.timestamp,
        });
        canonical_json_bytes(&unsigned)
    }

    pub fn compute_content_hash(&self) -> String {
        blake3_hex(&self.canonical_bytes())
    }

    pub fn verify_integrity(&self) -> Result<(), EventError> {
        let computed = self.compute_content_hash();
        if self.content_hash != computed {
            return Err(EventError::HashMismatch);
        }
        if self.event_id != self.content_hash {
            return Err(EventError::EventIdMismatch);
        }
        Ok(())
    }

    pub fn verify_signature(&self, public_key_hex: &str) -> Result<(), EventError> {
        verify(public_key_hex, &self.canonical_bytes(), &self.signature).map_err(EventError::from)
    }

    pub fn verify(&self, public_key_hex: &str) -> Result<(), EventError> {
        self.verify_integrity()?;
        self.verify_signature(public_key_hex)
    }
}

pub fn create_signed_event(
    identity: &crate::identity::Identity,
    event_type: impl Into<String>,
    protocol: impl Into<String>,
    logical_clock: u64,
    sequence_number: u64,
    timestamp: impl Into<String>,
    parents: Vec<String>,
    payload: Value,
    evidence_refs: Vec<String>,
    resource_refs: Vec<String>,
) -> Event {
    let mut event = Event {
        event_id: String::new(),
        event_type: event_type.into(),
        protocol: protocol.into(),
        actor_id: identity.agent_id.clone(),
        logical_clock,
        sequence_number,
        timestamp: timestamp.into(),
        parents,
        payload,
        evidence_refs,
        resource_refs,
        signature: String::new(),
        content_hash: String::new(),
    };
    let hash = event.compute_content_hash();
    event.event_id = hash.clone();
    event.content_hash = hash;
    event.signature = identity.sign(&event.canonical_bytes());
    event
}
