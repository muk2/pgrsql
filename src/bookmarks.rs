use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedQuery {
    pub name: String,
    pub query: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub last_used: Option<DateTime<Utc>>,
    #[serde(default)]
    pub is_builtin: bool,
}

pub fn load_bookmarks() -> Result<Vec<SavedQuery>> {
    let path = bookmark_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = std::fs::read_to_string(&path)?;
    let bookmarks: Vec<SavedQuery> = serde_json::from_str(&data)?;
    Ok(bookmarks)
}

pub fn save_bookmarks(bookmarks: &[SavedQuery]) -> Result<()> {
    let path = bookmark_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Only save non-builtin bookmarks
    let user_bookmarks: Vec<&SavedQuery> = bookmarks.iter().filter(|b| !b.is_builtin).collect();
    let data = serde_json::to_string_pretty(&user_bookmarks)?;
    std::fs::write(&path, data)?;
    Ok(())
}

fn bookmark_path() -> Result<std::path::PathBuf> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;
    Ok(config_dir.join("pgrsql").join("bookmarks.json"))
}

pub fn built_in_snippets() -> Vec<SavedQuery> {
    let now = Utc::now();
    vec![
        SavedQuery {
            name: "Table sizes".to_string(),
            query: "SELECT schemaname || '.' || tablename AS table,\n       pg_size_pretty(pg_total_relation_size(schemaname || '.' || tablename)) AS total_size,\n       pg_size_pretty(pg_relation_size(schemaname || '.' || tablename)) AS data_size\nFROM pg_tables\nWHERE schemaname NOT IN ('pg_catalog', 'information_schema')\nORDER BY pg_total_relation_size(schemaname || '.' || tablename) DESC\nLIMIT 20;".to_string(),
            description: Some("Show largest tables by total size".to_string()),
            tags: vec!["maintenance".to_string()],
            created_at: now,
            last_used: None,
            is_builtin: true,
        },
        SavedQuery {
            name: "Running queries".to_string(),
            query: "SELECT pid, usename, datname, state,\n       now() - query_start AS duration,\n       LEFT(query, 100) AS query\nFROM pg_stat_activity\nWHERE state = 'active'\n  AND pid <> pg_backend_pid()\nORDER BY query_start;".to_string(),
            description: Some("Show currently running queries".to_string()),
            tags: vec!["admin".to_string()],
            created_at: now,
            last_used: None,
            is_builtin: true,
        },
        SavedQuery {
            name: "Index usage stats".to_string(),
            query: "SELECT schemaname, relname AS table, indexrelname AS index,\n       idx_scan AS scans,\n       pg_size_pretty(pg_relation_size(indexrelid)) AS size\nFROM pg_stat_user_indexes\nORDER BY idx_scan DESC\nLIMIT 20;".to_string(),
            description: Some("Show index usage statistics".to_string()),
            tags: vec!["performance".to_string()],
            created_at: now,
            last_used: None,
            is_builtin: true,
        },
        SavedQuery {
            name: "Unused indexes".to_string(),
            query: "SELECT schemaname, relname AS table, indexrelname AS index,\n       pg_size_pretty(pg_relation_size(indexrelid)) AS size\nFROM pg_stat_user_indexes\nWHERE idx_scan = 0\nORDER BY pg_relation_size(indexrelid) DESC;".to_string(),
            description: Some("Find indexes that have never been scanned".to_string()),
            tags: vec!["performance".to_string(), "maintenance".to_string()],
            created_at: now,
            last_used: None,
            is_builtin: true,
        },
        SavedQuery {
            name: "Lock monitoring".to_string(),
            query: "SELECT l.pid, l.locktype, l.mode, l.granted,\n       a.usename, a.datname,\n       LEFT(a.query, 80) AS query\nFROM pg_locks l\nJOIN pg_stat_activity a ON l.pid = a.pid\nWHERE NOT l.granted\nORDER BY a.query_start;".to_string(),
            description: Some("Show blocked lock requests".to_string()),
            tags: vec!["admin".to_string()],
            created_at: now,
            last_used: None,
            is_builtin: true,
        },
        SavedQuery {
            name: "Cache hit ratio".to_string(),
            query: "SELECT datname,\n       ROUND(blks_hit * 100.0 / NULLIF(blks_hit + blks_read, 0), 2) AS cache_hit_ratio\nFROM pg_stat_database\nWHERE datname NOT LIKE 'template%'\nORDER BY cache_hit_ratio;".to_string(),
            description: Some("Show cache hit ratio per database".to_string()),
            tags: vec!["performance".to_string()],
            created_at: now,
            last_used: None,
            is_builtin: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_built_in_snippets_not_empty() {
        let snippets = built_in_snippets();
        assert!(!snippets.is_empty());
        assert_eq!(snippets.len(), 6);
    }

    #[test]
    fn test_built_in_snippets_are_marked_builtin() {
        for snippet in built_in_snippets() {
            assert!(snippet.is_builtin);
            assert!(!snippet.name.is_empty());
            assert!(!snippet.query.is_empty());
        }
    }

    #[test]
    fn test_saved_query_serialization() {
        let query = SavedQuery {
            name: "Test".to_string(),
            query: "SELECT 1;".to_string(),
            description: Some("test query".to_string()),
            tags: vec!["test".to_string()],
            created_at: Utc::now(),
            last_used: None,
            is_builtin: false,
        };
        let json = serde_json::to_string(&query).unwrap();
        let deserialized: SavedQuery = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "Test");
        assert_eq!(deserialized.query, "SELECT 1;");
        assert!(!deserialized.is_builtin);
    }

    #[test]
    fn test_saved_query_default_builtin() {
        // Test that is_builtin defaults to false when deserializing
        let json = r#"{"name":"Test","query":"SELECT 1;","description":null,"tags":[],"created_at":"2024-01-01T00:00:00Z","last_used":null}"#;
        let query: SavedQuery = serde_json::from_str(json).unwrap();
        assert!(!query.is_builtin);
    }

    #[test]
    fn test_built_in_snippets_have_descriptions() {
        for snippet in built_in_snippets() {
            assert!(snippet.description.is_some());
        }
    }

    #[test]
    fn test_built_in_snippets_have_tags() {
        for snippet in built_in_snippets() {
            assert!(!snippet.tags.is_empty());
        }
    }
}
