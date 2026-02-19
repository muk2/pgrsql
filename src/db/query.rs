use anyhow::Result;
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use std::time::{Duration, Instant};
use tokio_postgres::{types::Type, Client, Row};

#[derive(Debug, Clone)]
pub struct QueryResult {
    pub columns: Vec<ColumnInfo>,
    pub rows: Vec<Vec<CellValue>>,
    pub row_count: usize,
    pub execution_time: Duration,
    pub affected_rows: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ColumnInfo {
    pub name: String,
    pub type_name: String,
    pub max_width: usize,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum CellValue {
    Null,
    Bool(bool),
    Int16(i16),
    Int32(i32),
    Int64(i64),
    Float32(f32),
    Float64(f64),
    Text(String),
    Bytes(Vec<u8>),
    Date(NaiveDate),
    Time(NaiveTime),
    DateTime(NaiveDateTime),
    TimestampTz(DateTime<Utc>),
    Json(serde_json::Value),
    Array(Vec<CellValue>),
    Unknown(String),
}

impl CellValue {
    pub fn display(&self) -> String {
        match self {
            CellValue::Null => "NULL".to_string(),
            CellValue::Bool(b) => b.to_string(),
            CellValue::Int16(i) => i.to_string(),
            CellValue::Int32(i) => i.to_string(),
            CellValue::Int64(i) => i.to_string(),
            CellValue::Float32(f) => f.to_string(),
            CellValue::Float64(f) => f.to_string(),
            CellValue::Text(s) => s.clone(),
            CellValue::Bytes(b) => format!("[{} bytes]", b.len()),
            CellValue::Date(d) => d.to_string(),
            CellValue::Time(t) => t.to_string(),
            CellValue::DateTime(dt) => dt.to_string(),
            CellValue::TimestampTz(dt) => dt.to_string(),
            CellValue::Json(j) => j.to_string(),
            CellValue::Array(arr) => {
                let items: Vec<String> = arr.iter().map(|v| v.display()).collect();
                format!("{{{}}}", items.join(", "))
            }
            CellValue::Unknown(s) => s.clone(),
        }
    }

    pub fn display_width(&self) -> usize {
        unicode_width::UnicodeWidthStr::width(self.display().as_str())
    }

    /// Compare two CellValues for sorting purposes.
    /// Returns an Ordering suitable for sort operations.
    /// NULLs are always sorted last regardless of direction.
    pub fn sort_cmp(&self, other: &CellValue) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        match (self, other) {
            // NULLs always last
            (CellValue::Null, CellValue::Null) => Ordering::Equal,
            (CellValue::Null, _) => Ordering::Greater,
            (_, CellValue::Null) => Ordering::Less,

            // Booleans: false < true
            (CellValue::Bool(a), CellValue::Bool(b)) => a.cmp(b),

            // Integers
            (CellValue::Int16(a), CellValue::Int16(b)) => a.cmp(b),
            (CellValue::Int32(a), CellValue::Int32(b)) => a.cmp(b),
            (CellValue::Int64(a), CellValue::Int64(b)) => a.cmp(b),

            // Cross-integer comparison: promote to i64
            (CellValue::Int16(a), CellValue::Int32(b)) => (*a as i64).cmp(&(*b as i64)),
            (CellValue::Int32(a), CellValue::Int16(b)) => (*a as i64).cmp(&(*b as i64)),
            (CellValue::Int16(a), CellValue::Int64(b)) => (*a as i64).cmp(b),
            (CellValue::Int64(a), CellValue::Int16(b)) => a.cmp(&(*b as i64)),
            (CellValue::Int32(a), CellValue::Int64(b)) => (*a as i64).cmp(b),
            (CellValue::Int64(a), CellValue::Int32(b)) => a.cmp(&(*b as i64)),

            // Floats
            (CellValue::Float32(a), CellValue::Float32(b)) => {
                a.partial_cmp(b).unwrap_or(Ordering::Equal)
            }
            (CellValue::Float64(a), CellValue::Float64(b)) => {
                a.partial_cmp(b).unwrap_or(Ordering::Equal)
            }
            (CellValue::Float32(a), CellValue::Float64(b)) => {
                (*a as f64).partial_cmp(b).unwrap_or(Ordering::Equal)
            }
            (CellValue::Float64(a), CellValue::Float32(b)) => {
                a.partial_cmp(&(*b as f64)).unwrap_or(Ordering::Equal)
            }

            // Numeric vs float: promote int to f64
            (CellValue::Int16(a), CellValue::Float64(b)) => {
                (*a as f64).partial_cmp(b).unwrap_or(Ordering::Equal)
            }
            (CellValue::Float64(a), CellValue::Int16(b)) => {
                a.partial_cmp(&(*b as f64)).unwrap_or(Ordering::Equal)
            }
            (CellValue::Int32(a), CellValue::Float64(b)) => {
                (*a as f64).partial_cmp(b).unwrap_or(Ordering::Equal)
            }
            (CellValue::Float64(a), CellValue::Int32(b)) => {
                a.partial_cmp(&(*b as f64)).unwrap_or(Ordering::Equal)
            }
            (CellValue::Int64(a), CellValue::Float64(b)) => {
                (*a as f64).partial_cmp(b).unwrap_or(Ordering::Equal)
            }
            (CellValue::Float64(a), CellValue::Int64(b)) => {
                a.partial_cmp(&(*b as f64)).unwrap_or(Ordering::Equal)
            }

            // Text
            (CellValue::Text(a), CellValue::Text(b)) => a.cmp(b),

            // Dates and times
            (CellValue::Date(a), CellValue::Date(b)) => a.cmp(b),
            (CellValue::Time(a), CellValue::Time(b)) => a.cmp(b),
            (CellValue::DateTime(a), CellValue::DateTime(b)) => a.cmp(b),
            (CellValue::TimestampTz(a), CellValue::TimestampTz(b)) => a.cmp(b),

            // Fallback: compare display strings
            (a, b) => a.display().cmp(&b.display()),
        }
    }
}

#[allow(dead_code)]
impl QueryResult {
    pub fn empty() -> Self {
        Self {
            columns: vec![],
            rows: vec![],
            row_count: 0,
            execution_time: Duration::ZERO,
            affected_rows: None,
            error: None,
        }
    }

    pub fn error(msg: String, execution_time: Duration) -> Self {
        Self {
            columns: vec![],
            rows: vec![],
            row_count: 0,
            execution_time,
            affected_rows: None,
            error: Some(msg),
        }
    }
}

pub async fn execute_query(client: &Client, sql: &str) -> Result<QueryResult> {
    let start = Instant::now();
    let sql_trimmed = sql.trim();

    // Check if it's a SELECT-like query or a modification query
    let sql_upper = sql_trimmed.to_uppercase();
    let is_select = sql_upper.starts_with("SELECT")
        || sql_upper.starts_with("WITH")
        || sql_upper.starts_with("SHOW")
        || sql_upper.starts_with("EXPLAIN")
        || sql_upper.starts_with("TABLE");

    if is_select {
        match client.query(sql_trimmed, &[]).await {
            Ok(rows) => {
                let execution_time = start.elapsed();
                let result = parse_rows(&rows, execution_time);
                Ok(result)
            }
            Err(e) => {
                let execution_time = start.elapsed();
                Ok(QueryResult::error(e.to_string(), execution_time))
            }
        }
    } else {
        match client.execute(sql_trimmed, &[]).await {
            Ok(affected) => {
                let execution_time = start.elapsed();
                Ok(QueryResult {
                    columns: vec![],
                    rows: vec![],
                    row_count: 0,
                    execution_time,
                    affected_rows: Some(affected),
                    error: None,
                })
            }
            Err(e) => {
                let execution_time = start.elapsed();
                Ok(QueryResult::error(e.to_string(), execution_time))
            }
        }
    }
}

fn parse_rows(rows: &[Row], execution_time: Duration) -> QueryResult {
    if rows.is_empty() {
        return QueryResult {
            columns: vec![],
            rows: vec![],
            row_count: 0,
            execution_time,
            affected_rows: None,
            error: None,
        };
    }

    let first_row = &rows[0];
    let columns: Vec<ColumnInfo> = first_row
        .columns()
        .iter()
        .map(|col| ColumnInfo {
            name: col.name().to_string(),
            type_name: col.type_().name().to_string(),
            max_width: col.name().len(),
        })
        .collect();

    let mut result_rows: Vec<Vec<CellValue>> = Vec::with_capacity(rows.len());

    for row in rows {
        let mut row_values: Vec<CellValue> = Vec::with_capacity(columns.len());

        for (i, col) in row.columns().iter().enumerate() {
            let value = extract_value(row, i, col.type_());
            row_values.push(value);
        }

        result_rows.push(row_values);
    }

    // Calculate max widths
    let mut columns = columns;
    for row in &result_rows {
        for (i, cell) in row.iter().enumerate() {
            let width = cell.display_width();
            if width > columns[i].max_width {
                columns[i].max_width = width;
            }
        }
    }

    let row_count = result_rows.len();

    QueryResult {
        columns,
        rows: result_rows,
        row_count,
        execution_time,
        affected_rows: None,
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- CellValue display ---

    #[test]
    fn test_null_display() {
        assert_eq!(CellValue::Null.display(), "NULL");
    }

    #[test]
    fn test_bool_display() {
        assert_eq!(CellValue::Bool(true).display(), "true");
        assert_eq!(CellValue::Bool(false).display(), "false");
    }

    #[test]
    fn test_integer_display() {
        assert_eq!(CellValue::Int16(42).display(), "42");
        assert_eq!(CellValue::Int32(-100).display(), "-100");
        assert_eq!(CellValue::Int64(9_999_999).display(), "9999999");
    }

    #[test]
    fn test_float_display() {
        assert_eq!(CellValue::Float32(3.14).display(), "3.14");
        assert_eq!(CellValue::Float64(2.718).display(), "2.718");
    }

    #[test]
    fn test_text_display() {
        assert_eq!(CellValue::Text("hello".into()).display(), "hello");
    }

    #[test]
    fn test_bytes_display() {
        assert_eq!(CellValue::Bytes(vec![1, 2, 3]).display(), "[3 bytes]");
    }

    #[test]
    fn test_json_display() {
        let val = serde_json::json!({"key": "value"});
        let display = CellValue::Json(val).display();
        assert!(display.contains("key"));
        assert!(display.contains("value"));
    }

    #[test]
    fn test_array_display() {
        let arr = CellValue::Array(vec![
            CellValue::Int32(1),
            CellValue::Int32(2),
            CellValue::Int32(3),
        ]);
        assert_eq!(arr.display(), "{1, 2, 3}");
    }

    #[test]
    fn test_unknown_display() {
        assert_eq!(CellValue::Unknown("raw".into()).display(), "raw");
    }

    // --- CellValue display_width ---

    #[test]
    fn test_display_width() {
        assert_eq!(CellValue::Null.display_width(), 4); // "NULL"
        assert_eq!(CellValue::Text("hello".into()).display_width(), 5);
        assert_eq!(CellValue::Int32(100).display_width(), 3);
    }

    // --- QueryResult ---

    #[test]
    fn test_empty_result() {
        let r = QueryResult::empty();
        assert!(r.columns.is_empty());
        assert!(r.rows.is_empty());
        assert_eq!(r.row_count, 0);
        assert!(r.error.is_none());
        assert!(r.affected_rows.is_none());
    }

    #[test]
    fn test_error_result() {
        let r = QueryResult::error("bad query".into(), Duration::from_millis(10));
        assert!(r.error.is_some());
        assert_eq!(r.error.unwrap(), "bad query");
        assert!(r.rows.is_empty());
    }

    // --- CellValue sort_cmp ---

    #[test]
    fn test_sort_cmp_nulls_last() {
        use std::cmp::Ordering;
        assert_eq!(CellValue::Null.sort_cmp(&CellValue::Null), Ordering::Equal);
        assert_eq!(
            CellValue::Null.sort_cmp(&CellValue::Int32(1)),
            Ordering::Greater
        );
        assert_eq!(
            CellValue::Int32(1).sort_cmp(&CellValue::Null),
            Ordering::Less
        );
    }

    #[test]
    fn test_sort_cmp_integers() {
        use std::cmp::Ordering;
        assert_eq!(
            CellValue::Int32(1).sort_cmp(&CellValue::Int32(2)),
            Ordering::Less
        );
        assert_eq!(
            CellValue::Int32(5).sort_cmp(&CellValue::Int32(5)),
            Ordering::Equal
        );
        assert_eq!(
            CellValue::Int64(100).sort_cmp(&CellValue::Int64(50)),
            Ordering::Greater
        );
    }

    #[test]
    fn test_sort_cmp_cross_integer() {
        use std::cmp::Ordering;
        assert_eq!(
            CellValue::Int16(10).sort_cmp(&CellValue::Int32(20)),
            Ordering::Less
        );
        assert_eq!(
            CellValue::Int32(30).sort_cmp(&CellValue::Int64(30)),
            Ordering::Equal
        );
    }

    #[test]
    fn test_sort_cmp_floats() {
        use std::cmp::Ordering;
        assert_eq!(
            CellValue::Float64(1.5).sort_cmp(&CellValue::Float64(2.5)),
            Ordering::Less
        );
        assert_eq!(
            CellValue::Float32(3.0).sort_cmp(&CellValue::Float64(3.0)),
            Ordering::Equal
        );
    }

    #[test]
    fn test_sort_cmp_text() {
        use std::cmp::Ordering;
        assert_eq!(
            CellValue::Text("apple".into()).sort_cmp(&CellValue::Text("banana".into())),
            Ordering::Less
        );
        assert_eq!(
            CellValue::Text("zebra".into()).sort_cmp(&CellValue::Text("aardvark".into())),
            Ordering::Greater
        );
    }

    #[test]
    fn test_sort_cmp_booleans() {
        use std::cmp::Ordering;
        assert_eq!(
            CellValue::Bool(false).sort_cmp(&CellValue::Bool(true)),
            Ordering::Less
        );
    }

    #[test]
    fn test_sort_stable_with_nulls() {
        let mut values = vec![
            CellValue::Int32(3),
            CellValue::Null,
            CellValue::Int32(1),
            CellValue::Null,
            CellValue::Int32(2),
        ];
        values.sort_by(|a, b| a.sort_cmp(b));
        // Nulls should be at the end
        assert_eq!(values[0].display(), "1");
        assert_eq!(values[1].display(), "2");
        assert_eq!(values[2].display(), "3");
        assert_eq!(values[3].display(), "NULL");
        assert_eq!(values[4].display(), "NULL");
    }
}

fn extract_value(row: &Row, idx: usize, pg_type: &Type) -> CellValue {
    // Try to extract based on type
    match *pg_type {
        Type::BOOL => row
            .try_get::<_, Option<bool>>(idx)
            .ok()
            .flatten()
            .map(CellValue::Bool)
            .unwrap_or(CellValue::Null),
        Type::INT2 => row
            .try_get::<_, Option<i16>>(idx)
            .ok()
            .flatten()
            .map(CellValue::Int16)
            .unwrap_or(CellValue::Null),
        Type::INT4 => row
            .try_get::<_, Option<i32>>(idx)
            .ok()
            .flatten()
            .map(CellValue::Int32)
            .unwrap_or(CellValue::Null),
        Type::INT8 => row
            .try_get::<_, Option<i64>>(idx)
            .ok()
            .flatten()
            .map(CellValue::Int64)
            .unwrap_or(CellValue::Null),
        Type::FLOAT4 => row
            .try_get::<_, Option<f32>>(idx)
            .ok()
            .flatten()
            .map(CellValue::Float32)
            .unwrap_or(CellValue::Null),
        Type::FLOAT8 | Type::NUMERIC => row
            .try_get::<_, Option<f64>>(idx)
            .ok()
            .flatten()
            .map(CellValue::Float64)
            .unwrap_or(CellValue::Null),
        Type::TEXT | Type::VARCHAR | Type::NAME | Type::CHAR | Type::BPCHAR => row
            .try_get::<_, Option<String>>(idx)
            .ok()
            .flatten()
            .map(CellValue::Text)
            .unwrap_or(CellValue::Null),
        Type::BYTEA => row
            .try_get::<_, Option<Vec<u8>>>(idx)
            .ok()
            .flatten()
            .map(CellValue::Bytes)
            .unwrap_or(CellValue::Null),
        Type::DATE => row
            .try_get::<_, Option<NaiveDate>>(idx)
            .ok()
            .flatten()
            .map(CellValue::Date)
            .unwrap_or(CellValue::Null),
        Type::TIME => row
            .try_get::<_, Option<NaiveTime>>(idx)
            .ok()
            .flatten()
            .map(CellValue::Time)
            .unwrap_or(CellValue::Null),
        Type::TIMESTAMP => row
            .try_get::<_, Option<NaiveDateTime>>(idx)
            .ok()
            .flatten()
            .map(CellValue::DateTime)
            .unwrap_or(CellValue::Null),
        Type::TIMESTAMPTZ => row
            .try_get::<_, Option<DateTime<Utc>>>(idx)
            .ok()
            .flatten()
            .map(CellValue::TimestampTz)
            .unwrap_or(CellValue::Null),
        Type::JSON | Type::JSONB => row
            .try_get::<_, Option<serde_json::Value>>(idx)
            .ok()
            .flatten()
            .map(CellValue::Json)
            .unwrap_or(CellValue::Null),
        _ => {
            // Fallback: try to get as string
            row.try_get::<_, Option<String>>(idx)
                .ok()
                .flatten()
                .map(CellValue::Text)
                .unwrap_or(CellValue::Null)
        }
    }
}
