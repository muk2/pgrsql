use crate::db::{CellValue, QueryResult};

pub fn to_csv(result: &QueryResult) -> String {
    let mut output = String::new();

    // Header
    let headers: Vec<String> = result.columns.iter().map(|c| csv_escape(&c.name)).collect();
    output.push_str(&headers.join(","));
    output.push('\n');

    // Rows
    for row in &result.rows {
        let cells: Vec<String> = row
            .iter()
            .map(|cell| csv_escape(&cell_to_csv(cell)))
            .collect();
        output.push_str(&cells.join(","));
        output.push('\n');
    }

    output
}

pub fn to_json(result: &QueryResult) -> String {
    let mut rows_json: Vec<serde_json::Value> = Vec::new();

    for row in &result.rows {
        let mut obj = serde_json::Map::new();
        for (i, cell) in row.iter().enumerate() {
            let col_name = result
                .columns
                .get(i)
                .map(|c| c.name.clone())
                .unwrap_or_else(|| format!("column_{}", i));
            obj.insert(col_name, cell_to_json(cell));
        }
        rows_json.push(serde_json::Value::Object(obj));
    }

    serde_json::to_string_pretty(&rows_json).unwrap_or_else(|_| "[]".to_string())
}

pub fn to_sql_insert(result: &QueryResult, table_name: &str) -> String {
    if result.rows.is_empty() || result.columns.is_empty() {
        return String::new();
    }

    let mut output = String::new();
    let col_names: Vec<&str> = result.columns.iter().map(|c| c.name.as_str()).collect();

    for row in &result.rows {
        output.push_str(&format!(
            "INSERT INTO {} ({}) VALUES\n",
            table_name,
            col_names.join(", ")
        ));
        let values: Vec<String> = row.iter().map(cell_to_sql).collect();
        output.push_str(&format!("  ({});\n", values.join(", ")));
    }

    output
}

pub fn to_tsv(result: &QueryResult) -> String {
    let mut output = String::new();

    // Header
    let headers: Vec<&str> = result.columns.iter().map(|c| c.name.as_str()).collect();
    output.push_str(&headers.join("\t"));
    output.push('\n');

    // Rows
    for row in &result.rows {
        let cells: Vec<String> = row
            .iter()
            .map(|cell| cell_to_csv(cell).replace('\t', " "))
            .collect();
        output.push_str(&cells.join("\t"));
        output.push('\n');
    }

    output
}

fn cell_to_csv(cell: &CellValue) -> String {
    match cell {
        CellValue::Null => String::new(),
        other => other.display(),
    }
}

fn cell_to_json(cell: &CellValue) -> serde_json::Value {
    match cell {
        CellValue::Null => serde_json::Value::Null,
        CellValue::Bool(b) => serde_json::Value::Bool(*b),
        CellValue::Int16(i) => serde_json::json!(*i),
        CellValue::Int32(i) => serde_json::json!(*i),
        CellValue::Int64(i) => serde_json::json!(*i),
        CellValue::Float32(f) => serde_json::json!(*f),
        CellValue::Float64(f) => serde_json::json!(*f),
        CellValue::Json(j) => j.clone(),
        CellValue::Array(arr) => {
            let items: Vec<serde_json::Value> = arr.iter().map(cell_to_json).collect();
            serde_json::Value::Array(items)
        }
        other => serde_json::Value::String(other.display()),
    }
}

fn cell_to_sql(cell: &CellValue) -> String {
    match cell {
        CellValue::Null => "NULL".to_string(),
        CellValue::Bool(b) => {
            if *b {
                "TRUE".to_string()
            } else {
                "FALSE".to_string()
            }
        }
        CellValue::Int16(i) => i.to_string(),
        CellValue::Int32(i) => i.to_string(),
        CellValue::Int64(i) => i.to_string(),
        CellValue::Float32(f) => f.to_string(),
        CellValue::Float64(f) => f.to_string(),
        CellValue::Text(s) => format!("'{}'", s.replace('\'', "''")),
        CellValue::Json(j) => format!("'{}'", j.to_string().replace('\'', "''")),
        other => format!("'{}'", other.display().replace('\'', "''")),
    }
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{CellValue, ColumnInfo, QueryResult};
    use std::time::Duration;

    fn make_result() -> QueryResult {
        QueryResult {
            columns: vec![
                ColumnInfo {
                    name: "id".to_string(),
                    type_name: "int4".to_string(),
                    max_width: 2,
                },
                ColumnInfo {
                    name: "name".to_string(),
                    type_name: "text".to_string(),
                    max_width: 10,
                },
                ColumnInfo {
                    name: "active".to_string(),
                    type_name: "bool".to_string(),
                    max_width: 5,
                },
            ],
            rows: vec![
                vec![
                    CellValue::Int32(1),
                    CellValue::Text("Alice".to_string()),
                    CellValue::Bool(true),
                ],
                vec![
                    CellValue::Int32(2),
                    CellValue::Text("Bob".to_string()),
                    CellValue::Null,
                ],
            ],
            row_count: 2,
            execution_time: Duration::from_millis(10),
            affected_rows: None,
            error: None,
        }
    }

    #[test]
    fn test_csv_export() {
        let result = make_result();
        let csv = to_csv(&result);
        assert!(csv.starts_with("id,name,active\n"));
        assert!(csv.contains("1,Alice,true\n"));
        assert!(csv.contains("2,Bob,\n"));
    }

    #[test]
    fn test_csv_escaping() {
        assert_eq!(csv_escape("hello"), "hello");
        assert_eq!(csv_escape("hello,world"), "\"hello,world\"");
        assert_eq!(csv_escape("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn test_json_export() {
        let result = make_result();
        let json = to_json(&result);
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0]["id"], 1);
        assert_eq!(parsed[0]["name"], "Alice");
        assert_eq!(parsed[0]["active"], true);
        assert!(parsed[1]["active"].is_null());
    }

    #[test]
    fn test_sql_insert_export() {
        let result = make_result();
        let sql = to_sql_insert(&result, "users");
        assert!(sql.contains("INSERT INTO users (id, name, active) VALUES"));
        assert!(sql.contains("(1, 'Alice', TRUE)"));
        assert!(sql.contains("(2, 'Bob', NULL)"));
    }

    #[test]
    fn test_tsv_export() {
        let result = make_result();
        let tsv = to_tsv(&result);
        assert!(tsv.starts_with("id\tname\tactive\n"));
        assert!(tsv.contains("1\tAlice\ttrue\n"));
    }

    #[test]
    fn test_empty_result_sql_insert() {
        let result = QueryResult::empty();
        let sql = to_sql_insert(&result, "users");
        assert!(sql.is_empty());
    }

    #[test]
    fn test_sql_single_quote_escaping() {
        assert_eq!(
            cell_to_sql(&CellValue::Text("O'Brien".to_string())),
            "'O''Brien'"
        );
    }

    #[test]
    fn test_json_null_handling() {
        let json = cell_to_json(&CellValue::Null);
        assert!(json.is_null());
    }

    #[test]
    fn test_json_number_types() {
        assert_eq!(cell_to_json(&CellValue::Int32(42)), serde_json::json!(42));
        assert_eq!(
            cell_to_json(&CellValue::Bool(true)),
            serde_json::json!(true)
        );
    }
}
