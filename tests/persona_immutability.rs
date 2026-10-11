//! Pins the claim that identity/persona semantics cannot be rewritten by an
//! agent-originated event. `persona` is not part of `WorldState`, so replay can
//! never mutate it; these tests assert the complementary kernel-level guard —
//! that an agent cannot even submit an event *labelled* as a persona/identity
//! mutation and have it accepted.

use molt_kernel::agent::Action;
use molt_kernel::identity::Identity;
use molt_kernel::llm_agent::{LlmAgentShell, MockClient};
use molt_kernel::memory::InMemoryStore;
use molt_kernel::runtime::{is_reserved_event_type, LocalWorld};
use molt_kernel::store::{EventStore, MemoryEventStore};
use serde_json::json;

fn world_with_agent(id: &str) -> LocalWorld<MemoryEventStore, InMemoryStore> {
    let mut world =
        LocalWorld::new("test-world", MemoryEventStore::new(), InMemoryStore::default());
    let identity = Identity::generate(id);
    let shell = LlmAgentShell::new(
        id,
        "baseline persona",
        vec![],
        Box::new(MockClient {
            fixed_reply: "hi".into(),
        }),
    );
    world.register_agent(identity, Box::new(shell)).unwrap();
    world
}

#[test]
fn reserved_set_covers_identity_and_persona_types() {
    assert!(is_reserved_event_type("GENESIS_CEREMONY"));
    assert!(is_reserved_event_type("PERSONA_UPDATED"));
    assert!(!is_reserved_event_type("DESIGN_PROPOSED"));
}

#[test]
fn agent_cannot_propose_a_persona_mutation_event() {
    let mut world = world_with_agent("A01");
    let result = world.commit_action(
        "A01",
        Action::Propose {
            event_type: "PERSONA_UPDATED".into(),
            payload: json!({ "persona": "overwritten" }),
        },
    );
    assert!(result.is_err(), "persona mutation proposal must be rejected");
    // Nothing was appended: the store is still empty.
    assert!(world.store.all().unwrap().is_empty());
}

#[test]
fn ordinary_agent_proposals_are_still_accepted() {
    let mut world = world_with_agent("A01");
    let event = world
        .commit_action(
            "A01",
            Action::Propose {
                event_type: "DESIGN_PROPOSED".into(),
                payload: json!({ "bridge": "candidate-1" }),
            },
        )
        .unwrap();
    assert_eq!(event.event_type, "DESIGN_PROPOSED");
}
