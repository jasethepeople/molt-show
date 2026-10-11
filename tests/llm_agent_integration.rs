use molt_kernel::genesis::create_genesis;
use molt_kernel::identity::Identity;
use molt_kernel::llm_agent::{LlmAgentShell, LlmClient};
use molt_kernel::memory::FileMemoryStore;
use molt_kernel::runtime::{LocalWorld, WorldError};
use molt_kernel::store::{EventStore, FileEventStore};
use std::time::{SystemTime, UNIX_EPOCH};

/// Deterministic stand-in for a real model call — returns whatever line it
/// was constructed with, every turn. Lets these tests exercise the real
/// `WorldAgent` / `LocalWorld` / signed-event path without touching the
/// network.
struct Canned(String);
impl LlmClient for Canned {
    fn complete(&self, _system_prompt: &str, _transcript: &str) -> Result<String, WorldError> {
        Ok(self.0.clone())
    }
}

fn temp_path(label: &str) -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("molt-llm-agent-{label}-{nonce}.jsonl"))
}

#[test]
fn two_llm_shells_exchange_real_signed_events() {
    let event_path = temp_path("events");
    let memory_path = temp_path("memory");

    let attester = Identity::generate("attester");
    let a01 = Identity::generate("A01");
    let a02 = Identity::generate("A02");

    let store = FileEventStore::open(&event_path).unwrap();
    let memory = FileMemoryStore::open(&memory_path).unwrap();
    let mut world = LocalWorld::new("test-world", store, memory);

    create_genesis(
        &attester,
        "MOLT/0.1",
        &[a01.clone(), a02.clone()],
        "2026-09-18T00:00:00Z",
    );

    world
        .register_agent(
            a01.clone(),
            Box::new(LlmAgentShell::new(
                "A01",
                "persona A01",
                vec!["A02".into()],
                Box::new(Canned("hello from A01".into())),
            )),
        )
        .unwrap();
    world
        .register_agent(
            a02.clone(),
            Box::new(LlmAgentShell::new(
                "A02",
                "persona A02",
                vec!["A01".into()],
                Box::new(Canned("hello back from A02".into())),
            )),
        )
        .unwrap();

    let e1 = world.decide_and_commit("A01").unwrap();
    assert_eq!(e1.event_type, "MESSAGE_SENT");
    assert_eq!(e1.payload["recipient_id"], "A02");
    assert_eq!(e1.payload["body"]["text"], "hello from A01");
    world.receive_next("A02").unwrap();

    let e2 = world.decide_and_commit("A02").unwrap();
    assert_eq!(e2.payload["body"]["text"], "hello back from A02");
    world.receive_next("A01").unwrap();

    // Every committed action, including these LLM-shell turns, is a
    // properly signed event that passed through FileEventStore::append's
    // own invariant checks — not a bypass around the kernel's own rules.
    // (If a shell somehow produced an unsigned or malformed event, append
    // itself would have already rejected it before we got here.)
    let all_events = world.store.all().unwrap();
    let message_events: Vec<_> = all_events
        .iter()
        .filter(|e| e.event_type == "MESSAGE_SENT")
        .collect();
    assert_eq!(message_events.len(), 2);
    for event in &message_events {
        assert!(!event.signature.is_empty());
        assert!(!event.content_hash.is_empty());
    }

    std::fs::remove_file(&event_path).unwrap();
    std::fs::remove_file(&memory_path).unwrap();
}

#[test]
fn kernel_visibility_is_global_even_though_llm_shells_filter_their_own_transcript() {
    let event_path = temp_path("private-events");
    let memory_path = temp_path("private-memory");

    let attester = Identity::generate("attester");
    let a01 = Identity::generate("A01");
    let a02 = Identity::generate("A02");
    let a03 = Identity::generate("A03");

    let store = FileEventStore::open(&event_path).unwrap();
    let memory = FileMemoryStore::open(&memory_path).unwrap();
    let mut world = LocalWorld::new("test-world", store, memory);
    create_genesis(
        &attester,
        "MOLT/0.1",
        &[a01.clone(), a02.clone(), a03.clone()],
        "2026-09-18T00:00:00Z",
    );

    world
        .register_agent(
            a01.clone(),
            Box::new(LlmAgentShell::new(
                "A01",
                "p",
                vec!["A02".into()],
                Box::new(Canned("a message meant for A02".into())),
            )),
        )
        .unwrap();
    world
        .register_agent(
            a02.clone(),
            Box::new(LlmAgentShell::new(
                "A02",
                "p",
                vec!["A01".into()],
                Box::new(Canned("acknowledged".into())),
            )),
        )
        .unwrap();
    world
        .register_agent(
            a03.clone(),
            Box::new(LlmAgentShell::new(
                "A03",
                "p",
                vec![],
                Box::new(Canned("unused".into())),
            )),
        )
        .unwrap();

    world.decide_and_commit("A01").unwrap();

    // A03 never sent or received anything, but the kernel's own visibility
    // is global: A03's observation still contains A01's message. (Whether
    // an LlmAgentShell's *own* transcript-builder chooses to filter that
    // down to what touches it is a separate, already unit-tested policy —
    // this test only pins the kernel-level guarantee underneath it.)
    let observation = world.observe("A03").unwrap();
    let visible_message_events = observation
        .context
        .visible_events
        .iter()
        .filter(|e| e.event_type == "MESSAGE_SENT")
        .count();
    assert_eq!(visible_message_events, 1);

    std::fs::remove_file(&event_path).unwrap();
    std::fs::remove_file(&memory_path).unwrap();
}

#[test]
fn identities_round_trip_through_export_and_reconstruction() {
    // This is the mechanism `converse`'s resume feature relies on: export a
    // signing key, reconstruct an Identity from the raw bytes later, and
    // confirm it produces byte-identical signatures — i.e. it really is
    // the same cryptographic identity, not a look-alike.
    let original = Identity::generate("A01");
    let bytes = original.signing_key_bytes();
    let restored = Identity::from_signing_key_bytes("A01", &bytes);

    assert_eq!(original.public_key_hex(), restored.public_key_hex());
    let message = b"resume me";
    assert_eq!(original.sign(message), restored.sign(message));
}
