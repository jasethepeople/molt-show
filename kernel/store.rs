use crate::events::Event;
use crate::invariants::{validate_history, InvariantViolation};
use crate::world::{replay, ReplayError, StateSnapshot};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

pub type EventId = String;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("event violates history invariant: {0}")]
    Invariant(#[from] InvariantViolation),
    #[error("event store I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("event serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("event replay error: {0}")]
    Replay(#[from] ReplayError),
}

pub trait EventStore {
    fn append(&mut self, event: Event) -> Result<(), StoreError>;
    fn get(&self, id: &EventId) -> Result<Option<Event>, StoreError>;
    fn parents(&self, id: &EventId) -> Result<Vec<Event>, StoreError>;
    fn stream(&self, agent: &str, from: u64, to: u64) -> Result<Vec<Event>, StoreError>;
    fn all(&self) -> Result<Vec<Event>, StoreError>;
    fn snapshot(&self) -> Result<StateSnapshot, StoreError>;
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryEventStore {
    events: Vec<Event>,
}

impl MemoryEventStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl EventStore for MemoryEventStore {
    fn append(&mut self, event: Event) -> Result<(), StoreError> {
        validate_history(&event, &self.events)?;
        self.events.push(event);
        Ok(())
    }
    fn get(&self, id: &EventId) -> Result<Option<Event>, StoreError> {
        Ok(self
            .events
            .iter()
            .find(|event| &event.event_id == id)
            .cloned())
    }
    fn parents(&self, id: &EventId) -> Result<Vec<Event>, StoreError> {
        let event = self.get(id)?;
        Ok(event
            .map(|event| {
                event
                    .parents
                    .iter()
                    .filter_map(|parent| {
                        self.events
                            .iter()
                            .find(|candidate| &candidate.event_id == parent)
                            .cloned()
                    })
                    .collect()
            })
            .unwrap_or_default())
    }
    fn stream(&self, agent: &str, from: u64, to: u64) -> Result<Vec<Event>, StoreError> {
        Ok(self
            .events
            .iter()
            .filter(|event| {
                event.actor_id == agent
                    && event.sequence_number >= from
                    && event.sequence_number <= to
            })
            .cloned()
            .collect())
    }
    fn all(&self) -> Result<Vec<Event>, StoreError> {
        Ok(self.events.clone())
    }
    fn snapshot(&self) -> Result<StateSnapshot, StoreError> {
        Ok(replay(&self.events)?.snapshot())
    }
}

#[derive(Debug, Clone)]
pub struct FileEventStore {
    path: PathBuf,
}

impl FileEventStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let store = Self {
            path: path.as_ref().to_path_buf(),
        };
        if !store.path.exists() {
            File::create(&store.path)?;
        }
        let _ = store.all()?;
        Ok(store)
    }
}

impl EventStore for FileEventStore {
    fn append(&mut self, event: Event) -> Result<(), StoreError> {
        let events = self.all()?;
        validate_history(&event, &events)?;
        let mut file = OpenOptions::new().append(true).open(&self.path)?;
        serde_json::to_writer(&mut file, &event)?;
        file.write_all(b"\n")?;
        file.sync_data()?;
        Ok(())
    }
    fn get(&self, id: &EventId) -> Result<Option<Event>, StoreError> {
        Ok(self.all()?.into_iter().find(|event| &event.event_id == id))
    }
    fn parents(&self, id: &EventId) -> Result<Vec<Event>, StoreError> {
        let events = self.all()?;
        Ok(events
            .iter()
            .find(|event| &event.event_id == id)
            .map(|event| {
                event
                    .parents
                    .iter()
                    .filter_map(|parent| {
                        events
                            .iter()
                            .find(|candidate| &candidate.event_id == parent)
                            .cloned()
                    })
                    .collect()
            })
            .unwrap_or_default())
    }
    fn stream(&self, agent: &str, from: u64, to: u64) -> Result<Vec<Event>, StoreError> {
        Ok(self
            .all()?
            .into_iter()
            .filter(|event| {
                event.actor_id == agent
                    && event.sequence_number >= from
                    && event.sequence_number <= to
            })
            .collect())
    }
    fn all(&self) -> Result<Vec<Event>, StoreError> {
        let file = File::open(&self.path)?;
        BufReader::new(file)
            .lines()
            .filter(|line| {
                line.as_ref()
                    .map(|line| !line.trim().is_empty())
                    .unwrap_or(true)
            })
            .map(|line| Ok(serde_json::from_str(&line?)?))
            .collect()
    }
    fn snapshot(&self) -> Result<StateSnapshot, StoreError> {
        Ok(replay(&self.all()?)?.snapshot())
    }
}
