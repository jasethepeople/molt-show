use molt_kernel::events::{create_signed_event, Event};
use molt_kernel::identity::Identity;
use molt_kernel::memory::{InMemoryStore, MemoryKind, MemoryRecord, MemoryStore};
use molt_kernel::observatory::Observatory;
use molt_kernel::store::{EventStore, MemoryEventStore};
use molt_kernel::world::replay;
use serde_json::json;

fn event(
    identity: &Identity,
    kind: &str,
    seq: u64,
    clock: u64,
    parent: Option<&Event>,
    payload: serde_json::Value,
) -> Event {
    create_signed_event(
        identity,
        kind,
        "MOLT/0.1",
        clock,
        seq,
        "2026-09-18T00:00:00Z",
        parent.map(|e| vec![e.event_id.clone()]).unwrap_or_default(),
        payload,
        vec![],
        vec![],
    )
}

#[test]
fn observatory_queries_match_replayed_history() {
    let identity = Identity::generate("A17");
    let first = event(
        &identity,
        "RESOURCE_POOL_CREATED",
        0,
        0,
        None,
        json!({"pool_id":"p","resource_type":"compute","owner_id":"A17","capacity":5}),
    );
    let second = event(
        &identity,
        "CLAIM_CREATED",
        1,
        1,
        Some(&first),
        json!({"claim_id":"C481","statement":"x"}),
    );
    let third = event(
        &identity,
        "EVIDENCE_SUBMITTED",
        2,
        2,
        Some(&second),
        json!({"claim_id":"C481","evidence_id":"E41"}),
    );
    let mut store = MemoryEventStore::new();
    for event in [&first, &second, &third] {
        store.append(event.clone()).unwrap();
    }
    let mut memory = InMemoryStore::default();
    memory
        .remember(MemoryRecord {
            memory_id: "m1".into(),
            agent_id: "A17".into(),
            kind: MemoryKind::Semantic,
            content: json!({"claim":"x"}),
            source_event: Some(second.event_id.clone()),
        })
        .unwrap();
    let observatory = Observatory::new(&store, &memory);
    assert_eq!(observatory.history(Some("A17"), None).unwrap().len(), 3);
    assert_eq!(observatory.claims(Some("A17")).unwrap().len(), 1);
    assert_eq!(observatory.evidence(Some("A17")).unwrap().len(), 1);
    assert_eq!(
        observatory
            .memory("A17", Some(MemoryKind::Semantic))
            .unwrap()
            .len(),
        1
    );
    let lineage = observatory.lineage(&third.event_id).unwrap();
    assert_eq!(lineage.ancestors.len(), 2);
    assert_eq!(lineage.descendants.len(), 0);
    assert_eq!(
        observatory.state_hash_at(&third.event_id).unwrap(),
        replay(&[first.clone(), second.clone(), third.clone()])
            .unwrap()
            .hash()
    );
    assert!(observatory.verify_history().unwrap().valid);
}

#[test]
fn observatory_is_read_only() {
    let identity = Identity::generate("A17");
    let event = event(
        &identity,
        "MESSAGE_SENT",
        0,
        0,
        None,
        json!({"recipient_id":"A42"}),
    );
    let mut store = MemoryEventStore::new();
    store.append(event).unwrap();
    let memory = InMemoryStore::default();
    let observatory = Observatory::new(&store, &memory);
    let before = store.all().unwrap();
    let _ = observatory.history(None, Some("MESSAGE_SENT")).unwrap();
    assert_eq!(store.all().unwrap(), before);
}
