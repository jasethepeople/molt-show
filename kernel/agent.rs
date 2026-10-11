use crate::events::Event;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorldContext {
    pub world_id: String,
    pub state_hash: String,
    pub visible_events: Vec<Event>,
    pub available_capabilities: Vec<String>,
    pub available_resources: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Observation {
    pub observer_id: String,
    pub context: WorldContext,
    pub claims: Vec<Claim>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Claim {
    pub statement: String,
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Action {
    UseCapability {
        capability_id: String,
        input: serde_json::Value,
    },
    SendMessage(Message),
    Remember {
        kind: crate::memory::MemoryKind,
        content: serde_json::Value,
    },
    Propose {
        event_type: String,
        payload: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Message {
    pub message_id: String,
    pub sender_id: String,
    pub recipient_id: String,
    pub kind: MessageKind,
    pub body: serde_json::Value,
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageKind {
    Send,
    Request,
    Proposal,
    Challenge,
    Evidence,
    Offer,
    Accept,
    Reject,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Capability {
    pub capability_id: String,
    pub interface: String,
    pub version: String,
    pub requirements: Vec<String>,
    pub authorized_agents: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityResult {
    pub capability_id: String,
    pub agent_id: String,
    pub output: serde_json::Value,
    pub consumed_resources: Vec<String>,
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("capability {0} is not authorized")]
    UnauthorizedCapability(String),
    #[error("agent action is invalid: {0}")]
    InvalidAction(String),
    #[error("agent memory error: {0}")]
    Memory(String),
    #[error("agent message error: {0}")]
    Message(String),
}

pub trait WorldAgent {
    fn agent_id(&self) -> &str;
    fn observe(&self, context: &WorldContext) -> Result<Observation, AgentError>;
    fn decide(&mut self, observation: &Observation) -> Result<Action, AgentError>;
    fn receive(&mut self, message: Message) -> Result<(), AgentError>;
}
