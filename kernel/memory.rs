use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MemoryKind {
    Episodic,
    Semantic,
    Evidence,
    Relationship,
    Lineage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryRecord {
    pub memory_id: String,
    pub agent_id: String,
    pub kind: MemoryKind,
    pub content: serde_json::Value,
    pub source_event: Option<String>,
}

#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("memory I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("memory serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub trait MemoryStore {
    fn remember(&mut self, record: MemoryRecord) -> Result<(), MemoryError>;
    fn recall(
        &self,
        agent_id: &str,
        kind: Option<MemoryKind>,
    ) -> Result<Vec<MemoryRecord>, MemoryError>;
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InMemoryStore {
    records: Vec<MemoryRecord>,
}
impl MemoryStore for InMemoryStore {
    fn remember(&mut self, record: MemoryRecord) -> Result<(), MemoryError> {
        self.records.push(record);
        Ok(())
    }
    fn recall(
        &self,
        agent_id: &str,
        kind: Option<MemoryKind>,
    ) -> Result<Vec<MemoryRecord>, MemoryError> {
        Ok(self
            .records
            .iter()
            .filter(|r| {
                r.agent_id == agent_id && kind.as_ref().map(|k| &r.kind == k).unwrap_or(true)
            })
            .cloned()
            .collect())
    }
}

#[derive(Debug, Clone)]
pub struct FileMemoryStore {
    path: PathBuf,
}
impl FileMemoryStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, MemoryError> {
        let store = Self {
            path: path.as_ref().to_path_buf(),
        };
        if !store.path.exists() {
            File::create(&store.path)?;
        }
        let _ = store.all()?;
        Ok(store)
    }
    fn all(&self) -> Result<Vec<MemoryRecord>, MemoryError> {
        let file = File::open(&self.path)?;
        BufReader::new(file)
            .lines()
            .filter(|l| l.as_ref().map(|x| !x.trim().is_empty()).unwrap_or(true))
            .map(|l| Ok(serde_json::from_str(&l?)?))
            .collect()
    }
}
impl MemoryStore for FileMemoryStore {
    fn remember(&mut self, record: MemoryRecord) -> Result<(), MemoryError> {
        let mut file = OpenOptions::new().append(true).open(&self.path)?;
        serde_json::to_writer(&mut file, &record)?;
        file.write_all(b"\n")?;
        file.sync_data()?;
        Ok(())
    }
    fn recall(
        &self,
        agent_id: &str,
        kind: Option<MemoryKind>,
    ) -> Result<Vec<MemoryRecord>, MemoryError> {
        Ok(self
            .all()?
            .into_iter()
            .filter(|r| {
                r.agent_id == agent_id && kind.as_ref().map(|k| &r.kind == k).unwrap_or(true)
            })
            .collect())
    }
}
