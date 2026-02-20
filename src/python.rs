//! Optional Python bindings for pgrsql via PyO3.
//!
//! Provides a Python API for SQL parsing, AST analysis, query compilation,
//! and EXPLAIN plan parsing. Enabled with the `python` feature flag.
//!
//! ## Usage from Python
//!
//! ```python
//! import pgrsql
//!
//! # Parse SQL to AST and compile back
//! result = pgrsql.parse_sql("SELECT * FROM users WHERE age > 18")
//! print(result)  # Compiled SQL string
//!
//! # Format SQL
//! formatted = pgrsql.format_sql("select id,name from users where age>18")
//! print(formatted)
//!
//! # Analyze query structure
//! analysis = pgrsql.analyze_query("SELECT * FROM a JOIN b ON a.id = b.id")
//! print(analysis)  # {'has_select': True, 'has_joins': True, ...}
//!
//! # Parse EXPLAIN output
//! plan = pgrsql.parse_explain("Seq Scan on users (cost=0.00..35.50 rows=100 width=36)")
//! print(plan)
//! ```

use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::ast::{compile, parse_sql as ast_parse_sql, Optimizer};
use crate::explain;

/// Parse one or more SQL statements and compile them back to normalized SQL.
///
/// Args:
///     sql: SQL string containing one or more statements.
///
/// Returns:
///     A list of compiled SQL strings, one per statement.
///
/// Raises:
///     ValueError: If the SQL cannot be parsed.
#[pyfunction]
fn parse_sql(sql: &str) -> PyResult<Vec<String>> {
    let queries = ast_parse_sql(sql).map_err(|e| {
        pyo3::exceptions::PyValueError::new_err(format!("SQL parse error: {}", e))
    })?;
    Ok(queries.iter().map(compile).collect())
}

/// Parse a single SQL statement, optimize it, and compile back to SQL.
///
/// Args:
///     sql: A single SQL statement.
///
/// Returns:
///     The optimized, compiled SQL string.
///
/// Raises:
///     ValueError: If the SQL cannot be parsed or contains multiple statements.
#[pyfunction]
fn format_sql(sql: &str) -> PyResult<String> {
    let query = crate::ast::parse_single(sql).map_err(|e| {
        pyo3::exceptions::PyValueError::new_err(format!("SQL parse error: {}", e))
    })?;
    let optimizer = Optimizer::with_defaults();
    let optimized = optimizer.optimize(query).map_err(|e| {
        pyo3::exceptions::PyValueError::new_err(format!("Optimization error: {}", e))
    })?;
    Ok(compile(&optimized))
}

/// Analyze a SQL query and return structural metadata.
///
/// Args:
///     sql: A single SQL statement to analyze.
///
/// Returns:
///     A dictionary with boolean flags for detected features:
///     has_select, has_joins, has_aggregation, has_window_functions,
///     has_subqueries, has_cte, has_recursive_cte, has_set_operations,
///     has_json_operations, join_count, etc.
///
/// Raises:
///     ValueError: If the SQL cannot be parsed.
#[pyfunction]
fn analyze_query(py: Python<'_>, sql: &str) -> PyResult<Py<PyDict>> {
    let query = crate::ast::parse_single(sql).map_err(|e| {
        pyo3::exceptions::PyValueError::new_err(format!("SQL parse error: {}", e))
    })?;
    let analysis = crate::ast::analyze_query(&query);

    let dict = PyDict::new(py);
    dict.set_item("has_select", analysis.has_select)?;
    dict.set_item("has_insert", analysis.has_insert)?;
    dict.set_item("has_update", analysis.has_update)?;
    dict.set_item("has_delete", analysis.has_delete)?;
    dict.set_item("has_distinct", analysis.has_distinct)?;
    dict.set_item("has_joins", analysis.has_joins)?;
    dict.set_item("join_count", analysis.join_count)?;
    dict.set_item("has_aggregation", analysis.has_aggregation)?;
    dict.set_item("has_window_functions", analysis.has_window_functions)?;
    dict.set_item("has_subqueries", analysis.has_subqueries)?;
    dict.set_item("has_cte", analysis.has_cte)?;
    dict.set_item("has_recursive_cte", analysis.has_recursive_cte)?;
    dict.set_item("has_set_operations", analysis.has_set_operations)?;
    dict.set_item("has_json_operations", analysis.has_json_operations)?;
    Ok(dict.into())
}

/// Parse PostgreSQL EXPLAIN output into a structured representation.
///
/// Args:
///     text: The text output from EXPLAIN or EXPLAIN ANALYZE.
///
/// Returns:
///     A dictionary with plan details, or None if parsing fails.
///     Contains: node_type, estimated_cost, actual_time, planning_time,
///     execution_time, total_time, and children (recursive).
#[pyfunction]
fn parse_explain(py: Python<'_>, text: &str) -> PyResult<Option<Py<PyDict>>> {
    match explain::parse_explain_output(text) {
        Some(plan) => {
            let dict = PyDict::new(py);
            dict.set_item("node_type", &plan.root.node_type)?;
            if let Some((start, end)) = plan.root.estimated_cost {
                dict.set_item("estimated_cost_start", start)?;
                dict.set_item("estimated_cost_end", end)?;
            }
            if let Some((start, end)) = plan.root.actual_time {
                dict.set_item("actual_time_start", start)?;
                dict.set_item("actual_time_end", end)?;
            }
            if let Some(rows) = plan.root.estimated_rows {
                dict.set_item("estimated_rows", rows)?;
            }
            if let Some(rows) = plan.root.actual_rows {
                dict.set_item("actual_rows", rows)?;
            }
            if let Some(t) = plan.total_time {
                dict.set_item("total_time", t)?;
            }
            if let Some(t) = plan.planning_time {
                dict.set_item("planning_time", t)?;
            }
            if let Some(t) = plan.execution_time {
                dict.set_item("execution_time", t)?;
            }
            dict.set_item("children_count", plan.root.children.len())?;
            Ok(Some(dict.into()))
        }
        None => Ok(None),
    }
}

/// Check if a SQL string is an EXPLAIN query.
///
/// Args:
///     sql: The SQL string to check.
///
/// Returns:
///     True if the query starts with EXPLAIN.
#[pyfunction]
fn is_explain_query(sql: &str) -> bool {
    explain::is_explain_query(sql)
}

/// pgrsql Python module.
///
/// Provides SQL parsing, analysis, formatting, and EXPLAIN plan parsing
/// powered by pgrsql's Rust engine.
#[pymodule]
fn _pgrsql(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse_sql, m)?)?;
    m.add_function(wrap_pyfunction!(format_sql, m)?)?;
    m.add_function(wrap_pyfunction!(analyze_query, m)?)?;
    m.add_function(wrap_pyfunction!(parse_explain, m)?)?;
    m.add_function(wrap_pyfunction!(is_explain_query, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sql_simple() {
        let result = parse_sql("SELECT * FROM users").unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].contains("SELECT"));
        assert!(result[0].contains("users"));
    }

    #[test]
    fn test_parse_sql_multiple() {
        let result = parse_sql("SELECT 1; SELECT 2").unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_parse_sql_error() {
        let result = parse_sql("SELCT * FORM users");
        assert!(result.is_err());
    }

    #[test]
    fn test_format_sql() {
        let result = format_sql("SELECT id,name FROM users WHERE age>18").unwrap();
        assert!(result.contains("SELECT"));
        assert!(result.contains("WHERE"));
    }

    #[test]
    fn test_format_sql_error() {
        let result = format_sql("NOT VALID SQL AT ALL %%");
        assert!(result.is_err());
    }

    #[test]
    fn test_is_explain() {
        assert!(is_explain_query("EXPLAIN SELECT 1"));
        assert!(is_explain_query("explain analyze select * from t"));
        assert!(!is_explain_query("SELECT 1"));
    }
}
