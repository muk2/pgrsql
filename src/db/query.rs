use anyhow::Result;
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use std::error::Error as StdError;
use std::fmt;
use std::time::{Duration, Instant};
use tokio_postgres::{types::Type, Client, Row};

/// Categorized error types for SQL query failures.
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorCategory {
    /// Syntax errors (SQLSTATE class 42 - syntax_error, etc.)
    Syntax,
    /// Semantic errors (missing table/column, ambiguous reference)
    Semantic,
    /// Execution/runtime errors (division by zero, constraint violation)
    Execution,
    /// Transaction state errors (e.g., transaction aborted)
    Transaction,
    /// Connection/communication errors
    Connection,
    /// Unknown or unclassified errors
    Unknown,
}

impl fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorCategory::Syntax => write!(f, "Syntax Error"),
            ErrorCategory::Semantic => write!(f, "Semantic Error"),
            ErrorCategory::Execution => write!(f, "Execution Error"),
            ErrorCategory::Transaction => write!(f, "Transaction Error"),
            ErrorCategory::Connection => write!(f, "Connection Error"),
            ErrorCategory::Unknown => write!(f, "Error"),
        }
    }
}

/// Structured error with rich context from PostgreSQL error responses.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct StructuredError {
    /// Categorized error type
    pub category: ErrorCategory,
    /// PostgreSQL severity (ERROR, FATAL, etc.)
    pub severity: String,
    /// SQLSTATE error code (e.g., "42601" for syntax_error)
    pub code: String,
    /// Primary error message
    pub message: String,
    /// Optional detail providing more context
    pub detail: Option<String>,
    /// Optional hint suggesting a fix
    pub hint: Option<String>,
    /// Character position in the query where the error occurred (1-based byte offset)
    pub position: Option<u32>,
    /// Schema associated with the error
    pub schema: Option<String>,
    /// Table associated with the error
    pub table: Option<String>,
    /// Column associated with the error
    pub column: Option<String>,
    /// Constraint associated with the error
    pub constraint: Option<String>,
    /// Context/traceback (e.g., PL/pgSQL call stack)
    pub where_: Option<String>,
    /// Computed line number (1-based) from position, if available
    pub line: Option<usize>,
    /// Computed column number (1-based) from position, if available
    pub col: Option<usize>,
}

#[allow(dead_code)]
impl StructuredError {
    /// Create a StructuredError from a tokio_postgres error, using the query text
    /// to compute line/column from the byte position.
    pub fn from_pg_error(err: &tokio_postgres::Error, query: &str) -> Self {
        if let Some(db_err) = err.as_db_error() {
            let code_str = db_err.code().code().to_string();
            let category = categorize_sqlstate(&code_str);
            let position = db_err.position().and_then(|p| match p {
                tokio_postgres::error::ErrorPosition::Original(pos) => Some(*pos),
                tokio_postgres::error::ErrorPosition::Internal { .. } => None,
            });

            let (line, col) = if let Some(pos) = position {
                byte_offset_to_line_col(query, pos as usize)
            } else {
                (None, None)
            };

            StructuredError {
                category,
                severity: db_err.severity().to_string(),
                code: code_str,
                message: db_err.message().to_string(),
                detail: db_err.detail().map(|s| s.to_string()),
                hint: db_err.hint().map(|s| s.to_string()),
                position,
                schema: db_err.schema().map(|s| s.to_string()),
                table: db_err.table().map(|s| s.to_string()),
                column: db_err.column().map(|s| s.to_string()),
                constraint: db_err.constraint().map(|s| s.to_string()),
                where_: db_err.where_().map(|s| s.to_string()),
                line,
                col,
            }
        } else {
            // Non-database error (connection, protocol, etc.)
            let category = if err.source().is_some() {
                ErrorCategory::Connection
            } else {
                ErrorCategory::Unknown
            };
            StructuredError {
                category,
                severity: "ERROR".to_string(),
                code: String::new(),
                message: err.to_string(),
                detail: err.source().map(|e| e.to_string()),
                hint: None,
                position: None,
                schema: None,
                table: None,
                column: None,
                constraint: None,
                where_: None,
                line: None,
                col: None,
            }
        }
    }

    /// Create a simple error from a plain string (for non-database errors).
    pub fn from_string(msg: String) -> Self {
        StructuredError {
            category: ErrorCategory::Unknown,
            severity: "ERROR".to_string(),
            code: String::new(),
            message: msg,
            detail: None,
            hint: None,
            position: None,
            schema: None,
            table: None,
            column: None,
            constraint: None,
            where_: None,
            line: None,
            col: None,
        }
    }

    /// Format as a single display string (for status bar, history, etc.)
    pub fn display_message(&self) -> String {
        self.message.clone()
    }

    /// Format as a rich multi-line string for the results panel.
    pub fn display_full(&self) -> String {
        let mut lines = Vec::new();

        // Category + message
        lines.push(format!("{}: {}", self.category, self.message));

        // Line/column
        if let (Some(line), Some(col)) = (self.line, self.col) {
            lines.push(format!("  at line {}, column {}", line, col));
        }

        // SQLSTATE code
        if !self.code.is_empty() {
            lines.push(format!("  SQLSTATE: {}", self.code));
        }

        // Detail
        if let Some(detail) = &self.detail {
            lines.push(format!("  Detail: {}", detail));
        }

        // Hint
        if let Some(hint) = &self.hint {
            lines.push(format!("  Hint: {}", hint));
        }

        // Schema/table/column context
        if let Some(schema) = &self.schema {
            if let Some(table) = &self.table {
                if let Some(column) = &self.column {
                    lines.push(format!("  Object: {}.{}.{}", schema, table, column));
                } else {
                    lines.push(format!("  Object: {}.{}", schema, table));
                }
            }
        } else if let Some(table) = &self.table {
            lines.push(format!("  Table: {}", table));
        }

        // Constraint
        if let Some(constraint) = &self.constraint {
            lines.push(format!("  Constraint: {}", constraint));
        }

        // Where context
        if let Some(where_) = &self.where_ {
            lines.push(format!("  Context: {}", where_));
        }

        lines.join("\n")
    }
}

impl fmt::Display for StructuredError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_message())
    }
}

/// Convert a 1-based byte offset in a query string to (line, column) both 1-based.
fn byte_offset_to_line_col(query: &str, byte_pos: usize) -> (Option<usize>, Option<usize>) {
    if byte_pos == 0 || query.is_empty() {
        return (Some(1), Some(1));
    }
    let target = (byte_pos - 1).min(query.len()); // PostgreSQL positions are 1-based
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, ch) in query.char_indices() {
        if i >= target {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (Some(line), Some(col))
}

/// Categorize a SQLSTATE code into an ErrorCategory.
fn categorize_sqlstate(code: &str) -> ErrorCategory {
    if code.len() < 2 {
        return ErrorCategory::Unknown;
    }
    let class = &code[..2];
    match class {
        // Class 42: Syntax Error or Access Rule Violation
        "42" => {
            // 42601 = syntax_error, 42501 = insufficient_privilege
            if code == "42601" || code == "42000" {
                ErrorCategory::Syntax
            } else {
                // 42P01 = undefined_table, 42703 = undefined_column, etc.
                ErrorCategory::Semantic
            }
        }
        // Class 22: Data Exception (division by zero, etc.)
        "22" => ErrorCategory::Execution,
        // Class 23: Integrity Constraint Violation
        "23" => ErrorCategory::Execution,
        // Class 25: Invalid Transaction State
        "25" => ErrorCategory::Transaction,
        // Class 40: Transaction Rollback
        "40" => ErrorCategory::Transaction,
        // Class 08: Connection Exception
        "08" => ErrorCategory::Connection,
        // Class 53: Insufficient Resources
        "53" => ErrorCategory::Execution,
        // Class 54: Program Limit Exceeded
        "54" => ErrorCategory::Execution,
        // Class 55: Object Not In Prerequisite State
        "55" => ErrorCategory::Execution,
        // Class 57: Operator Intervention
        "57" => ErrorCategory::Execution,
        _ => ErrorCategory::Unknown,
    }
}

#[derive(Debug, Clone)]
pub struct QueryResult {
    pub columns: Vec<ColumnInfo>,
    pub rows: Vec<Vec<CellValue>>,
    pub row_count: usize,
    pub execution_time: Duration,
    pub affected_rows: Option<u64>,
    pub error: Option<StructuredError>,
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

    pub fn error(err: StructuredError, execution_time: Duration) -> Self {
        Self {
            columns: vec![],
            rows: vec![],
            row_count: 0,
            execution_time,
            affected_rows: None,
            error: Some(err),
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
                let structured = StructuredError::from_pg_error(&e, sql_trimmed);
                Ok(QueryResult::error(structured, execution_time))
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
                let structured = StructuredError::from_pg_error(&e, sql_trimmed);
                Ok(QueryResult::error(structured, execution_time))
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
        let r = QueryResult::error(
            StructuredError::from_string("bad query".into()),
            Duration::from_millis(10),
        );
        assert!(r.error.is_some());
        assert_eq!(r.error.as_ref().unwrap().message, "bad query");
        assert!(r.rows.is_empty());
    }

    #[test]
    fn test_structured_error_category_display() {
        assert_eq!(ErrorCategory::Syntax.to_string(), "Syntax Error");
        assert_eq!(ErrorCategory::Semantic.to_string(), "Semantic Error");
        assert_eq!(ErrorCategory::Execution.to_string(), "Execution Error");
        assert_eq!(ErrorCategory::Transaction.to_string(), "Transaction Error");
        assert_eq!(ErrorCategory::Connection.to_string(), "Connection Error");
        assert_eq!(ErrorCategory::Unknown.to_string(), "Error");
    }

    #[test]
    fn test_structured_error_from_string() {
        let err = StructuredError::from_string("test error".into());
        assert_eq!(err.category, ErrorCategory::Unknown);
        assert_eq!(err.message, "test error");
        assert!(err.detail.is_none());
        assert!(err.hint.is_none());
        assert!(err.position.is_none());
    }

    #[test]
    fn test_structured_error_display_full() {
        let err = StructuredError {
            category: ErrorCategory::Syntax,
            severity: "ERROR".to_string(),
            code: "42601".to_string(),
            message: "syntax error at or near \",\"".to_string(),
            detail: None,
            hint: Some("Remove trailing comma.".to_string()),
            position: Some(45),
            schema: None,
            table: None,
            column: None,
            constraint: None,
            where_: None,
            line: Some(3),
            col: Some(1),
        };
        let full = err.display_full();
        assert!(full.contains("Syntax Error"));
        assert!(full.contains("at line 3, column 1"));
        assert!(full.contains("42601"));
        assert!(full.contains("Remove trailing comma"));
    }

    #[test]
    fn test_byte_offset_to_line_col() {
        let query = "SELECT *\nFROM users\nWHERE id = 1";
        // Position 1 = 'S' on line 1, col 1
        assert_eq!(byte_offset_to_line_col(query, 1), (Some(1), Some(1)));
        // Position 10 = 'F' on line 2, col 1
        assert_eq!(byte_offset_to_line_col(query, 10), (Some(2), Some(1)));
        // Position 21 = 'W' on line 3, col 1
        assert_eq!(byte_offset_to_line_col(query, 21), (Some(3), Some(1)));
    }

    #[test]
    fn test_categorize_sqlstate() {
        assert_eq!(categorize_sqlstate("42601"), ErrorCategory::Syntax);
        assert_eq!(categorize_sqlstate("42P01"), ErrorCategory::Semantic);
        assert_eq!(categorize_sqlstate("42703"), ErrorCategory::Semantic);
        assert_eq!(categorize_sqlstate("23505"), ErrorCategory::Execution);
        assert_eq!(categorize_sqlstate("22012"), ErrorCategory::Execution);
        assert_eq!(categorize_sqlstate("25001"), ErrorCategory::Transaction);
        assert_eq!(categorize_sqlstate("08006"), ErrorCategory::Connection);
        assert_eq!(categorize_sqlstate("XX000"), ErrorCategory::Unknown);
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
