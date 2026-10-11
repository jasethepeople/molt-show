use crate::events::Event;
use std::collections::HashSet;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum InvariantViolation {
    #[error("I2: event failed integrity: {0}")]
    InvalidEvent(String),
    #[error("I3: event {0} is already accepted")]
    DuplicateEvent(String),
    #[error("I3: unknown parent event {0}")]
    UnknownParent(String),
    #[error("I3: event cannot be its own parent")]
    SelfParent,
    #[error("I3: sequence number is not monotonic: expected {expected}, got {got}")]
    Sequence { expected: u64, got: u64 },
    #[error("I3: logical clock {got} is less than required {minimum}")]
    LogicalClock { minimum: u64, got: u64 },
    #[error("I3: logical clock {got} is less than parent requirement {minimum}")]
    ParentLogicalClock { minimum: u64, got: u64 },
}

/// Validates ancestry/order against an already accepted event set.
pub fn validate_history(event: &Event, accepted: &[Event]) -> Result<(), InvariantViolation> {
    event
        .verify_integrity()
        .map_err(|error| InvariantViolation::InvalidEvent(error.to_string()))?;

    let known: HashSet<&str> = accepted.iter().map(|e| e.event_id.as_str()).collect();
    if known.contains(event.event_id.as_str()) {
        return Err(InvariantViolation::DuplicateEvent(event.event_id.clone()));
    }

    let mut max_parent_clock = None;
    for parent in &event.parents {
        if parent == &event.event_id {
            return Err(InvariantViolation::SelfParent);
        }
        if !known.contains(parent.as_str()) {
            return Err(InvariantViolation::UnknownParent(parent.clone()));
        }
        if let Some(parent_event) = accepted
            .iter()
            .find(|candidate| candidate.event_id == *parent)
        {
            max_parent_clock = Some(
                max_parent_clock
                    .unwrap_or(0)
                    .max(parent_event.logical_clock),
            );
        }
    }
    if let Some(parent_clock) = max_parent_clock {
        let minimum = parent_clock.saturating_add(1);
        if event.logical_clock < minimum {
            return Err(InvariantViolation::ParentLogicalClock {
                minimum,
                got: event.logical_clock,
            });
        }
    }
    if let Some(last) = accepted.iter().max_by_key(|e| e.sequence_number) {
        if event.sequence_number != last.sequence_number + 1 {
            return Err(InvariantViolation::Sequence {
                expected: last.sequence_number + 1,
                got: event.sequence_number,
            });
        }
        let minimum = last.logical_clock.saturating_add(1);
        if event.logical_clock < minimum {
            return Err(InvariantViolation::LogicalClock {
                minimum,
                got: event.logical_clock,
            });
        }
    }
    Ok(())
}
