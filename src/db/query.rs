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
pub struct ColumnInfo {
    pub name: String,
    pub type_name: String,
    pub max_width: usize,
}

#[derive(Debug, Clone)]
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
}

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
