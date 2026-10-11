use crate::agent::{
    Action, AgentError, Capability, CapabilityResult, Message, Observation, WorldAgent,
    WorldContext,
};
use crate::events::{create_signed_event, Event};
use crate::identity::Identity;
use crate::memory::{MemoryRecord, MemoryStore};
use crate::store::{EventStore, StoreError};
use crate::world::{replay, WorldState};
use std::collections::{BTreeMap, VecDeque};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WorldError {
    #[error("unknown agent {0}")]
    UnknownAgent(String),
    #[error("unknown capability {0}")]
    UnknownCapability(String),
    #[error("agent error: {0}")]
    Agent(#[from] AgentError),
    #[error("event store error: {0}")]
    Store(#[from] StoreError),
    #[error("capability execution error: {0}")]
    Capability(String),
}

pub trait CapabilityExecutor {
    fn execute(
        &self,
        agent_id: &str,
        input: &serde_json::Value,
    ) -> Result<CapabilityResult, WorldError>;
}

pub struct LocalWorld<S: EventStore, M: MemoryStore> {
    pub world_id: String,
    pub store: S,
    pub memory: M,
    identities: BTreeMap<String, Identity>,
    agents: BTreeMap<String, Box<dyn WorldAgent>>,
    capabilities: BTreeMap<String, (Capability, Box<dyn CapabilityExecutor>)>,
    inboxes: BTreeMap<String, VecDeque<Message>>,
}

impl<S: EventStore, M: MemoryStore> LocalWorld<S, M> {
    pub fn new(world_id: impl Into<String>, store: S, memory: M) -> Self {
        Self {
            world_id: world_id.into(),
            store,
            memory,
            identities: BTreeMap::new(),
            agents: BTreeMap::new(),
            capabilities: BTreeMap::new(),
            inboxes: BTreeMap::new(),
        }
    }

    pub fn register_agent(
        &mut self,
        identity: Identity,
        agent: Box<dyn WorldAgent>,
    ) -> Result<(), WorldError> {
        let id = identity.agent_id.clone();
        if agent.agent_id() != id {
            return Err(WorldError::Agent(AgentError::InvalidAction(
                "shell identity does not match cryptographic identity".into(),
            )));
        }
        self.inboxes.entry(id.clone()).or_default();
        self.identities.insert(id.clone(), identity);
        self.agents.insert(id, agent);
        Ok(())
    }

    pub fn register_capability(
        &mut self,
        capability: Capability,
        executor: Box<dyn CapabilityExecutor>,
    ) {
        self.capabilities
            .insert(capability.capability_id.clone(), (capability, executor));
    }

    pub fn observe(&self, agent_id: &str) -> Result<Observation, WorldError> {
        let agent = self
            .agents
            .get(agent_id)
            .ok_or_else(|| WorldError::UnknownAgent(agent_id.into()))?;
        let events = self.store.all()?;
        let state = replay(&events).map_err(|e| StoreError::Replay(e))?;
        let context = WorldContext {
            world_id: self.world_id.clone(),
            state_hash: state.hash(),
            visible_events: events,
            available_capabilities: self
                .capabilities
                .values()
                .filter(|(c, _)| c.authorized_agents.iter().any(|id| id == agent_id))
                .map(|(c, _)| c.capability_id.clone())
                .collect(),
            available_resources: state
                .resources
                .leases
                .values()
                .filter(|l| l.grantee_id == agent_id && l.available() > 0)
                .map(|l| l.lease_id.clone())
                .collect(),
        };
        agent.observe(&context).map_err(WorldError::Agent)
    }

    pub fn decide_and_commit(&mut self, agent_id: &str) -> Result<Event, WorldError> {
        let observation = self.observe(agent_id)?;
        let action = self
            .agents
            .get_mut(agent_id)
            .ok_or_else(|| WorldError::UnknownAgent(agent_id.into()))?
            .decide(&observation)?;
        self.commit_action(agent_id, action)
    }

    pub fn commit_action(&mut self, agent_id: &str, action: Action) -> Result<Event, WorldError> {
        let identity = self
            .identities
            .get(agent_id)
            .ok_or_else(|| WorldError::UnknownAgent(agent_id.into()))?;
        let existing = self.store.all()?;
        let sequence = existing.last().map(|e| e.sequence_number + 1).unwrap_or(0);
        let clock = existing.iter().map(|e| e.logical_clock).max().unwrap_or(0)
            + u64::from(!existing.is_empty());
        let parents = existing
            .last()
            .map(|e| vec![e.event_id.clone()])
            .unwrap_or_default();
        let capability_result = if let Action::UseCapability {
            capability_id,
            input,
        } = &action
        {
            let (capability, executor) = self
                .capabilities
                .get(capability_id)
                .ok_or_else(|| WorldError::UnknownCapability(capability_id.clone()))?;
            if !capability.authorized_agents.iter().any(|id| id == agent_id) {
                return Err(WorldError::Agent(AgentError::UnauthorizedCapability(
                    capability_id.clone(),
                )));
            }
            Some(executor.execute(agent_id, input)?)
        } else {
            None
        };
        let (event_type, payload, evidence_refs) = match &action {
            Action::SendMessage(message) => (
                "MESSAGE_SENT",
                serde_json::to_value(message)
                    .map_err(|e| WorldError::Agent(AgentError::Message(e.to_string())))?,
                message.evidence_refs.clone(),
            ),
            Action::Remember { kind, content } => (
                "MEMORY_RECORDED",
                serde_json::json!({"kind": kind, "content": content}),
                vec![],
            ),
            Action::Propose {
                event_type,
                payload,
            } => {
                if is_reserved_event_type(event_type) {
                    return Err(WorldError::Agent(AgentError::InvalidAction(format!(
                        "event type {event_type} is reserved and cannot be proposed by an agent"
                    ))));
                }
                (event_type.as_str(), payload.clone(), vec![])
            }
            Action::UseCapability {
                capability_id,
                input,
            } => (
                "CAPABILITY_INVOKED",
                serde_json::json!({"capability_id": capability_id, "input": input, "result": capability_result}),
                capability_result
                    .as_ref()
                    .map(|result| result.evidence_refs.clone())
                    .unwrap_or_default(),
            ),
        };
        let event = create_signed_event(
            identity,
            event_type,
            "MOLT/0.1",
            clock,
            sequence,
            &crate::clock::now_rfc3339(),
            parents,
            payload,
            evidence_refs,
            vec![],
        );
        self.store.append(event.clone())?;
        match action {
            Action::SendMessage(message) => {
                self.inboxes
                    .entry(message.recipient_id.clone())
                    .or_default()
                    .push_back(message);
            }
            Action::Remember { kind, content } => {
                self.memory
                    .remember(MemoryRecord {
                        memory_id: event.event_id.clone(),
                        agent_id: agent_id.into(),
                        kind,
                        content,
                        source_event: Some(event.event_id.clone()),
                    })
                    .map_err(|e| WorldError::Agent(AgentError::Memory(e.to_string())))?;
            }
            _ => {}
        }
        Ok(event)
    }

    pub fn receive_next(&mut self, agent_id: &str) -> Result<Option<Message>, WorldError> {
        let message = self
            .inboxes
            .get_mut(agent_id)
            .ok_or_else(|| WorldError::UnknownAgent(agent_id.into()))?
            .pop_front();
        if let Some(message) = message.clone() {
            self.agents
                .get_mut(agent_id)
                .ok_or_else(|| WorldError::UnknownAgent(agent_id.into()))?
                .receive(message)?;
        }
        Ok(message)
    }

    pub fn state(&self) -> Result<WorldState, WorldError> {
        Ok(replay(&self.store.all()?).map_err(StoreError::Replay)?)
    }
}

/// Event types that carry protocol/bootstrap/identity meaning. These may only
/// be produced by the kernel itself (e.g. the genesis ceremony), never minted
/// by an agent through `Action::Propose`.
///
/// This is the kernel-level check that keeps identity and persona semantics out
/// of reach of agent-originated events, rather than trusting callers to avoid
/// emitting them. Note the narrower truth it enforces: `persona` is not part of
/// `WorldState` at all, so no replayed event can mutate it. This guard exists so
/// an agent also cannot *label* an event as a persona/identity mutation and have
/// it accepted into the canonical log as though it were one.
pub fn is_reserved_event_type(event_type: &str) -> bool {
    matches!(
        event_type,
        "GENESIS_CEREMONY"
            | "IDENTITY_ISSUED"
            | "IDENTITY_REVOKED"
            | "PERSONA_SET"
            | "PERSONA_UPDATED"
    )
}
