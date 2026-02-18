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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_entry(query: &str) -> HistoryEntry {
        HistoryEntry {
            query: query.to_string(),
            timestamp: Utc::now(),
            database: "testdb".to_string(),
            execution_time_ms: 42,
            success: true,
        }
    }

    #[test]
    fn test_new_history_is_empty() {
        let h = QueryHistory::new();
        assert!(h.entries().is_empty());
    }

    #[test]
    fn test_add_entry() {
        let mut h = QueryHistory::new();
        h.add(make_entry("SELECT 1"));
        assert_eq!(h.entries().len(), 1);
        assert_eq!(h.entries()[0].query, "SELECT 1");
    }

    #[test]
    fn test_no_consecutive_duplicates() {
        let mut h = QueryHistory::new();
        h.add(make_entry("SELECT 1"));
        h.add(make_entry("SELECT 1"));
        assert_eq!(h.entries().len(), 1);
    }

    #[test]
    fn test_duplicate_with_whitespace_trimmed() {
        let mut h = QueryHistory::new();
        h.add(make_entry("SELECT 1"));
        h.add(make_entry("  SELECT 1  "));
        assert_eq!(h.entries().len(), 1);
    }

    #[test]
    fn test_non_consecutive_duplicates_allowed() {
        let mut h = QueryHistory::new();
        h.add(make_entry("SELECT 1"));
        h.add(make_entry("SELECT 2"));
        h.add(make_entry("SELECT 1"));
        assert_eq!(h.entries().len(), 3);
    }

    #[test]
    fn test_max_entries_trim() {
        let mut h = QueryHistory::new();
        // Override max for testing
        h.max_entries = 3;
        h.add(make_entry("q1"));
        h.add(make_entry("q2"));
        h.add(make_entry("q3"));
        h.add(make_entry("q4"));
        assert_eq!(h.entries().len(), 3);
        assert_eq!(h.entries()[0].query, "q2");
    }

    // --- Navigation ---

    #[test]
    fn test_previous_on_empty_returns_none() {
        let mut h = QueryHistory::new();
        assert!(h.previous().is_none());
    }

    #[test]
    fn test_previous_returns_last_entry() {
        let mut h = QueryHistory::new();
        h.add(make_entry("q1"));
        h.add(make_entry("q2"));
        let entry = h.previous().unwrap();
        assert_eq!(entry.query, "q2");
    }

    #[test]
    fn test_previous_navigates_backwards() {
        let mut h = QueryHistory::new();
        h.add(make_entry("q1"));
        h.add(make_entry("q2"));
        h.add(make_entry("q3"));
        assert_eq!(h.previous().unwrap().query, "q3");
        assert_eq!(h.previous().unwrap().query, "q2");
        assert_eq!(h.previous().unwrap().query, "q1");
    }

    #[test]
    fn test_previous_stops_at_first() {
        let mut h = QueryHistory::new();
        h.add(make_entry("q1"));
        h.add(make_entry("q2"));
        h.previous(); // q2
        h.previous(); // q1
        let entry = h.previous().unwrap(); // still q1
        assert_eq!(entry.query, "q1");
    }

    #[test]
    fn test_next_without_previous_returns_none() {
        let mut h = QueryHistory::new();
        h.add(make_entry("q1"));
        assert!(h.next().is_none());
    }

    #[test]
    fn test_next_navigates_forward() {
        let mut h = QueryHistory::new();
        h.add(make_entry("q1"));
        h.add(make_entry("q2"));
        h.add(make_entry("q3"));
        h.previous(); // q3
        h.previous(); // q2
        h.previous(); // q1
        assert_eq!(h.next().unwrap().query, "q2");
        assert_eq!(h.next().unwrap().query, "q3");
    }

    #[test]
    fn test_next_stops_at_last() {
        let mut h = QueryHistory::new();
        h.add(make_entry("q1"));
        h.add(make_entry("q2"));
        h.previous(); // q2
        h.previous(); // q1
        h.next(); // q2
        let entry = h.next().unwrap(); // still q2
        assert_eq!(entry.query, "q2");
    }

    #[test]
    fn test_reset_navigation() {
        let mut h = QueryHistory::new();
        h.add(make_entry("q1"));
        h.add(make_entry("q2"));
        h.previous(); // q2
        h.previous(); // q1
        h.reset_navigation();
        // After reset, previous() goes to last entry again
        assert_eq!(h.previous().unwrap().query, "q2");
    }

    #[test]
    fn test_add_resets_navigation() {
        let mut h = QueryHistory::new();
        h.add(make_entry("q1"));
        h.add(make_entry("q2"));
        h.previous(); // q2
        h.previous(); // q1
        h.add(make_entry("q3"));
        // After adding, navigation resets
        assert_eq!(h.previous().unwrap().query, "q3");
    }

    // --- Search ---

    #[test]
    fn test_search() {
        let mut h = QueryHistory::new();
        h.add(make_entry("SELECT * FROM users"));
        h.add(make_entry("INSERT INTO logs VALUES (1)"));
        h.add(make_entry("SELECT count(*) FROM users"));

        let results = h.search("select");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_search_case_insensitive() {
        let mut h = QueryHistory::new();
        h.add(make_entry("SELECT * FROM users"));
        let results = h.search("select");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_search_no_results() {
        let mut h = QueryHistory::new();
        h.add(make_entry("SELECT 1"));
        let results = h.search("UPDATE");
        assert!(results.is_empty());
    }

    // --- Serialization ---

    #[test]
    fn test_serialization_round_trip() {
        let mut h = QueryHistory::new();
        h.add(make_entry("SELECT 1"));
        h.add(make_entry("SELECT 2"));

        let json = serde_json::to_string(&h).unwrap();
        let deserialized: QueryHistory = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.entries().len(), 2);
        assert_eq!(deserialized.entries()[0].query, "SELECT 1");
    }
}
