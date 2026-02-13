use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub query: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub database: String,
    pub execution_time_ms: u64,
    pub success: bool,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct QueryHistory {
    entries: Vec<HistoryEntry>,
    #[serde(skip)]
    current_index: Option<usize>,
    max_entries: usize,
}

#[allow(dead_code)]
impl QueryHistory {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            current_index: None,
            max_entries: 1000,
        }
    }

    pub fn add(&mut self, entry: HistoryEntry) {
        // Don't add duplicate consecutive entries
        if let Some(last) = self.entries.last() {
            if last.query.trim() == entry.query.trim() {
                return;
            }
        }

        self.entries.push(entry);

        // Trim if too many entries
        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }

        self.current_index = None;
    }

    pub fn previous(&mut self) -> Option<&HistoryEntry> {
        if self.entries.is_empty() {
            return None;
        }

        let idx = match self.current_index {
            Some(i) if i > 0 => i - 1,
            Some(i) => i,
            None => self.entries.len() - 1,
        };

        self.current_index = Some(idx);
        self.entries.get(idx)
    }

    pub fn next(&mut self) -> Option<&HistoryEntry> {
        if self.entries.is_empty() {
            return None;
        }

        let idx = match self.current_index {
            Some(i) if i < self.entries.len() - 1 => i + 1,
            Some(i) => i,
            None => return None,
        };

        self.current_index = Some(idx);
        self.entries.get(idx)
    }

    pub fn reset_navigation(&mut self) {
        self.current_index = None;
    }

    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    pub fn search(&self, query: &str) -> Vec<&HistoryEntry> {
        let query_lower = query.to_lowercase();
        self.entries
            .iter()
            .filter(|e| e.query.to_lowercase().contains(&query_lower))
            .collect()
    }

    fn history_path() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("pgrsql")
            .join("history.json")
    }

    pub fn load() -> Result<Self> {
        let path = Self::history_path();
        if !path.exists() {
            return Ok(Self::new());
        }
        let content = std::fs::read_to_string(&path)?;
        let history: QueryHistory = serde_json::from_str(&content)?;
        Ok(history)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::history_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(&self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }
}
