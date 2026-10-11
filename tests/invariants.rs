use molt_kernel::events::create_signed_event;
use molt_kernel::genesis::{create_genesis, validate_genesis};
use molt_kernel::identity::Identity;
use molt_kernel::invariants::validate_history;
use serde_json::json;

#[test]
fn i1_identity_signature_verifies() {
    let id = Identity::generate("A1");
    let event = create_signed_event(
        &id,
        "TEST_EVENT",
        "MOLT/0.1",
        1,
        1,
        "2026-09-18T09:23:00Z",
        vec![],
        json!({"x": 1}),
        vec![],
        vec![],
    );
    assert!(event.verify(&id.public_key_hex()).is_ok());
}

#[test]
fn i2_events_are_immutable() {
    let id = Identity::generate("A1");
    let event = create_signed_event(
        &id,
        "TEST_EVENT",
        "MOLT/0.1",
        1,
        1,
        "2026-09-18T09:23:00Z",
        vec![],
        json!({"x": 1}),
        vec![],
        vec![],
    );
    let mut mutated = event.clone();
    mutated.payload["extra"] = json!("data");
    assert!(mutated.verify(&id.public_key_hex()).is_err());
}

#[test]
fn i3_parent_and_order_validation() {
    let id = Identity::generate("A1");
    let first = create_signed_event(
        &id,
        "FIRST",
        "MOLT/0.1",
        1,
        1,
        "2026-09-18T09:23:00Z",
        vec![],
        json!({}),
        vec![],
        vec![],
    );
    let second = create_signed_event(
        &id,
        "SECOND",
        "MOLT/0.1",
        2,
        2,
        "2026-09-18T09:24:00Z",
        vec![first.event_id.clone()],
        json!({}),
        vec![],
        vec![],
    );
    assert!(validate_history(&second, &[first]).is_ok());
}

#[test]
fn forged_signature_fails() {
    let signer = Identity::generate("A1");
    let other = Identity::generate("A2");
    let event = create_signed_event(
        &signer,
        "TEST_EVENT",
        "MOLT/0.1",
        1,
        1,
        "2026-09-18T09:23:00Z",
        vec![],
        json!({}),
        vec![],
        vec![],
    );
    assert!(event.verify(&other.public_key_hex()).is_err());
}

#[test]
fn genesis_is_valid_bootstrap() {
    let attester = Identity::generate("human_attester_1");
    let a1 = Identity::generate("A1");
    let a2 = Identity::generate("A2");
    let genesis = create_genesis(&attester, "MOLT/0.1.0", &[a1, a2], "2026-09-18T09:23:00Z");
    assert!(validate_genesis(&genesis).is_ok());
    assert!(genesis.verify(&attester.public_key_hex()).is_ok());
}

#[test]
fn canonical_hash_is_stable() {
    let id = Identity::generate("A1");
    let event = create_signed_event(
        &id,
        "TEST_EVENT",
        "MOLT/0.1",
        1,
        1,
        "2026-09-18T09:23:00Z",
        vec![],
        json!({"b": 2, "a": 1}),
        vec![],
        vec![],
    );
    assert_eq!(event.content_hash, event.compute_content_hash());
    assert_eq!(event.event_id, event.content_hash);
}

#[test]
fn duplicate_event_is_rejected() {
    let id = Identity::generate("A1");
    let first = create_signed_event(
        &id,
        "FIRST",
        "MOLT/0.1",
        1,
        1,
        "2026-09-18T09:23:00Z",
        vec![],
        json!({}),
        vec![],
        vec![],
    );
    assert!(matches!(
        validate_history(&first, std::slice::from_ref(&first)),
        Err(molt_kernel::invariants::InvariantViolation::DuplicateEvent(
            _
        ))
    ));
}

#[test]
fn tampered_event_is_rejected_by_history_validation() {
    let id = Identity::generate("A1");
    let first = create_signed_event(
        &id,
        "FIRST",
        "MOLT/0.1",
        1,
        1,
        "2026-09-18T09:23:00Z",
        vec![],
        json!({}),
        vec![],
        vec![],
    );
    let mut tampered = first.clone();
    tampered.payload = json!({"tampered": true});
    assert!(matches!(
        validate_history(&tampered, &[]),
        Err(molt_kernel::invariants::InvariantViolation::InvalidEvent(_))
    ));
}

#[test]
fn parent_clock_must_precede_child() {
    let id = Identity::generate("A1");
    let first = create_signed_event(
        &id,
        "FIRST",
        "MOLT/0.1",
        4,
        1,
        "2026-09-18T09:23:00Z",
        vec![],
        json!({}),
        vec![],
        vec![],
    );
    let second = create_signed_event(
        &id,
        "SECOND",
        "MOLT/0.1",
        4,
        2,
        "2026-09-18T09:24:00Z",
        vec![first.event_id.clone()],
        json!({}),
        vec![],
        vec![],
    );
    assert!(matches!(
        validate_history(&second, std::slice::from_ref(&first)),
        Err(molt_kernel::invariants::InvariantViolation::ParentLogicalClock { .. })
    ));
}
