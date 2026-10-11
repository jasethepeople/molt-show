use molt_kernel::events::{create_signed_event, Event};
use molt_kernel::experiment::{
    AuditLevel, CapabilityScope, ExperimentContext, InterventionPolicy, Objective, PrivacyScope,
    ResourceScope, TerminationCondition,
};
use molt_kernel::identity::Identity;
use molt_kernel::resources::ResourceError;
use molt_kernel::store::{EventStore, FileEventStore, MemoryEventStore};
use molt_kernel::world::{hash_state, replay};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

fn event(
    identity: &Identity,
    kind: &str,
    seq: u64,
    clock: u64,
    parents: Vec<String>,
    payload: serde_json::Value,
) -> Event {
    create_signed_event(
        identity,
        kind,
        "MOLT/0.1",
        clock,
        seq,
        "2026-09-18T10:00:00Z",
        parents,
        payload,
        vec![],
        vec![],
    )
}

fn history() -> (Identity, Vec<Event>) {
    let owner = Identity::generate("owner");
    let worker = Identity::generate("worker");
    let pool = event(
        &owner,
        "RESOURCE_POOL_CREATED",
        0,
        0,
        vec![],
        json!({"pool_id":"p1","resource_type":"compute","owner_id":"owner","capacity":1000}),
    );
    let lease = event(
        &owner,
        "RESOURCE_LEASE_ISSUED",
        1,
        1,
        vec![pool.event_id.clone()],
        json!({"lease_id":"l1","pool_id":"p1","grantee_id":"worker","amount":600}),
    );
    let sublease = event(
        &worker,
        "RESOURCE_LEASE_ISSUED",
        2,
        2,
        vec![lease.event_id.clone()],
        json!({"lease_id":"l2","pool_id":"p1","parent_lease_id":"l1","grantee_id":"child","amount":200}),
    );
    (owner, vec![pool, lease, sublease])
}

#[test]
fn destroy_and_reconstruct_world_has_identical_hash() {
    let (_, events) = history();
    let state_a = replay(&events).unwrap();
    let hash_a = hash_state(&state_a);
    let mut store = MemoryEventStore::new();
    for event in &events {
        store.append(event.clone()).unwrap();
    }
    let restored_events = store.all().unwrap();
    let state_b = replay(&restored_events).unwrap();
    assert_eq!(hash_a, hash_state(&state_b));
    assert_eq!(state_a.snapshot(), state_b.snapshot());
}

#[test]
fn durable_file_store_survives_reopen() {
    let (_, events) = history();
    let path = std::env::temp_dir().join(format!(
        "molt-alpha2-{}.jsonl",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    {
        let mut store = FileEventStore::open(&path).unwrap();
        for event in &events {
            store.append(event.clone()).unwrap();
        }
    }
    let reopened = FileEventStore::open(&path).unwrap();
    assert_eq!(reopened.all().unwrap(), events);
    assert_eq!(
        reopened.snapshot().unwrap().state_hash,
        replay(&events).unwrap().hash()
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn lease_conservation_rejects_overallocation_and_overconsumption() {
    let owner = Identity::generate("owner");
    let worker = Identity::generate("worker");
    let pool = event(
        &owner,
        "RESOURCE_POOL_CREATED",
        0,
        0,
        vec![],
        json!({"pool_id":"p1","resource_type":"compute","owner_id":"owner","capacity":1000}),
    );
    let lease = event(
        &owner,
        "RESOURCE_LEASE_ISSUED",
        1,
        1,
        vec![pool.event_id.clone()],
        json!({"lease_id":"l1","pool_id":"p1","grantee_id":"worker","amount":1000}),
    );
    let over = event(
        &worker,
        "RESOURCE_LEASE_ISSUED",
        3,
        3,
        vec![lease.event_id.clone()],
        json!({"lease_id":"l3","pool_id":"p1","parent_lease_id":"l1","grantee_id":"child","amount":401}),
    );
    let first_sublease = event(
        &worker,
        "RESOURCE_LEASE_ISSUED",
        2,
        2,
        vec![lease.event_id.clone()],
        json!({"lease_id":"l2","pool_id":"p1","parent_lease_id":"l1","grantee_id":"child","amount":600}),
    );
    let mut state = replay(&[pool.clone(), lease.clone(), first_sublease]).unwrap();
    let error = state.apply(&over).unwrap_err();
    assert!(matches!(
        error,
        molt_kernel::world::ReplayError::Resource {
            source: ResourceError::LeaseConstraint { .. },
            ..
        }
    ));

    let consumed = event(
        &worker,
        "RESOURCE_CONSUMED",
        2,
        2,
        vec![lease.event_id],
        json!({"lease_id":"l1","amount":1001}),
    );
    let error = state.apply(&consumed).unwrap_err();
    assert!(matches!(
        error,
        molt_kernel::world::ReplayError::Resource {
            source: ResourceError::ConsumptionConstraint { .. },
            ..
        }
    ));
}

#[test]
fn experiment_policy_metadata_is_hashable_and_immutable() {
    let context = ExperimentContext {
        experiment_id: "EMERGENCE-001".into(),
        objective: Objective {
            description: "Solve X".into(),
        },
        participants: vec!["A17".into(), "B42".into()],
        initial_state: "00".repeat(32),
        resource_scope: ResourceScope {
            pool_ids: vec!["p1".into()],
            maximum: Some(1000),
        },
        capability_scope: CapabilityScope {
            capabilities: vec!["compute".into()],
        },
        audit_policy: AuditLevel::ActionsAndEvidence,
        intervention_policy: InterventionPolicy::EmergencyStopOnly,
        privacy_scope: PrivacyScope::Participant,
        termination: TerminationCondition::Explicit,
        genesis_event: "genesis-id".into(),
    };
    let first = context.hash();
    let encoded = serde_json::to_vec(&context).unwrap();
    let decoded: ExperimentContext = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(first, decoded.hash());
}
