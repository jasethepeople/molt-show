use molt_kernel::agent::{
    Action, AgentError, Capability, CapabilityResult, Message, MessageKind, Observation,
    WorldAgent, WorldContext,
};
use molt_kernel::events::create_signed_event;
use molt_kernel::identity::Identity;
use molt_kernel::memory::{FileMemoryStore, MemoryKind, MemoryStore};
use molt_kernel::runtime::{CapabilityExecutor, LocalWorld, WorldError};
use molt_kernel::store::{EventStore, FileEventStore};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

struct ScriptedShell {
    id: String,
    actions: std::collections::VecDeque<Action>,
    received: Vec<Message>,
}
impl WorldAgent for ScriptedShell {
    fn agent_id(&self) -> &str {
        &self.id
    }
    fn observe(&self, context: &WorldContext) -> Result<Observation, AgentError> {
        Ok(Observation {
            observer_id: self.id.clone(),
            context: context.clone(),
            claims: vec![],
        })
    }
    fn decide(&mut self, _observation: &Observation) -> Result<Action, AgentError> {
        self.actions
            .pop_front()
            .ok_or_else(|| AgentError::InvalidAction("script exhausted".into()))
    }
    fn receive(&mut self, message: Message) -> Result<(), AgentError> {
        self.received.push(message);
        Ok(())
    }
}
struct EchoCapability;
impl CapabilityExecutor for EchoCapability {
    fn execute(
        &self,
        agent_id: &str,
        input: &serde_json::Value,
    ) -> Result<CapabilityResult, WorldError> {
        Ok(CapabilityResult {
            capability_id: "echo".into(),
            agent_id: agent_id.into(),
            output: input.clone(),
            consumed_resources: vec!["lease-a".into()],
            evidence_refs: vec!["evidence-1".into()],
        })
    }
}

#[test]
fn first_inhabitant_chain_survives_restart_and_replay() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let event_path = std::env::temp_dir().join(format!("molt-world-events-{nonce}.jsonl"));
    let memory_path = std::env::temp_dir().join(format!("molt-world-memory-{nonce}.jsonl"));
    let owner = Identity::generate("owner");
    let alice = Identity::generate("alice");
    let bob = Identity::generate("bob");
    {
        let mut store = FileEventStore::open(&event_path).unwrap();
        let pool = create_signed_event(
            &owner,
            "RESOURCE_POOL_CREATED",
            "MOLT/0.1",
            0,
            0,
            "2026-09-18T00:00:00Z",
            vec![],
            json!({"pool_id":"pool-a","resource_type":"compute","owner_id":"owner","capacity":10}),
            vec![],
            vec![],
        );
        let lease = create_signed_event(
            &owner,
            "RESOURCE_LEASE_ISSUED",
            "MOLT/0.1",
            1,
            1,
            "2026-09-18T00:00:00Z",
            vec![pool.event_id.clone()],
            json!({"lease_id":"lease-a","pool_id":"pool-a","grantee_id":"alice","amount":10}),
            vec![],
            vec![],
        );
        store.append(pool).unwrap();
        store.append(lease).unwrap();
        let memory = FileMemoryStore::open(&memory_path).unwrap();
        let mut world = LocalWorld::new("world-1", store, memory);
        world
            .register_agent(
                alice.clone(),
                Box::new(ScriptedShell {
                    id: "alice".into(),
                    actions: std::collections::VecDeque::from(vec![
                        Action::UseCapability {
                            capability_id: "echo".into(),
                            input: json!({"x": 1}),
                        },
                        Action::SendMessage(Message {
                            message_id: "m1".into(),
                            sender_id: "alice".into(),
                            recipient_id: "bob".into(),
                            kind: MessageKind::Evidence,
                            body: json!({"result": "ok"}),
                            evidence_refs: vec!["evidence-1".into()],
                        }),
                        Action::Remember {
                            kind: MemoryKind::Episodic,
                            content: json!({"note":"completed"}),
                        },
                    ]),
                    received: vec![],
                }),
            )
            .unwrap();
        world
            .register_agent(
                bob.clone(),
                Box::new(ScriptedShell {
                    id: "bob".into(),
                    actions: std::collections::VecDeque::new(),
                    received: vec![],
                }),
            )
            .unwrap();
        world.register_capability(
            Capability {
                capability_id: "echo".into(),
                interface: "echo.v1".into(),
                version: "1".into(),
                requirements: vec!["lease-a".into()],
                authorized_agents: vec!["alice".into()],
            },
            Box::new(EchoCapability),
        );
        let capability_event = world.decide_and_commit("alice").unwrap();
        assert_eq!(capability_event.event_type, "CAPABILITY_INVOKED");
        assert_eq!(capability_event.evidence_refs, vec!["evidence-1"]);
        assert_eq!(
            capability_event.payload["result"]["output"],
            json!({"x": 1})
        );
        world.decide_and_commit("alice").unwrap();
        world.decide_and_commit("alice").unwrap();
        assert_eq!(world.receive_next("bob").unwrap().unwrap().message_id, "m1");
        assert_eq!(world.state().unwrap().event_count, 5);
    }
    let reopened = FileEventStore::open(&event_path).unwrap();
    let restored = reopened.snapshot().unwrap();
    assert_eq!(restored.state.event_count, 5);
    let memory = FileMemoryStore::open(&memory_path).unwrap();
    assert_eq!(
        memory
            .recall("alice", Some(MemoryKind::Episodic))
            .unwrap()
            .len(),
        1
    );
    std::fs::remove_file(event_path).unwrap();
    std::fs::remove_file(memory_path).unwrap();
}

#[test]
fn unauthorized_capability_is_rejected_before_commit() {
    let path = std::env::temp_dir().join(format!(
        "molt-world-unauthorized-{}.jsonl",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let identity = Identity::generate("alice");
    let store = FileEventStore::open(&path).unwrap();
    let memory = FileMemoryStore::open(path.with_extension("memory.jsonl")).unwrap();
    let mut world = LocalWorld::new("world-1", store, memory);
    world
        .register_agent(
            identity,
            Box::new(ScriptedShell {
                id: "alice".into(),
                actions: std::collections::VecDeque::new(),
                received: vec![],
            }),
        )
        .unwrap();
    world.register_capability(
        Capability {
            capability_id: "private".into(),
            interface: "private.v1".into(),
            version: "1".into(),
            requirements: vec![],
            authorized_agents: vec!["bob".into()],
        },
        Box::new(EchoCapability),
    );
    let result = world.commit_action(
        "alice",
        Action::UseCapability {
            capability_id: "private".into(),
            input: json!({}),
        },
    );
    assert!(matches!(
        result,
        Err(WorldError::Agent(AgentError::UnauthorizedCapability(_)))
    ));
    assert!(world.store.all().unwrap().is_empty());
}
