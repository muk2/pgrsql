/// AST optimization and transformation infrastructure.
///
/// Provides a pass-based system for analyzing and transforming query ASTs.
/// Each optimization pass takes an AST, returns a potentially modified AST,
/// and preserves query semantics. Passes can be composed and ordered.
use anyhow::Result;

use super::types::*;

/// A single optimization or transformation pass over a query AST.
///
/// Passes should be pure functions: given the same input, they produce
/// the same output. This makes them composable and testable.
///
/// # Example
///
/// ```ignore
/// struct ConstantFolding;
///
/// impl OptimizationPass for ConstantFolding {
///     fn name(&self) -> &str { "constant_folding" }
///     fn transform(&self, query: Query) -> Result<Query> {
///         // Evaluate constant expressions at compile time
///         // e.g., WHERE 1 = 1 → (removed), WHERE 2 + 3 > 4 → WHERE TRUE
///     }
/// }
/// ```
pub trait OptimizationPass: Send + Sync {
    /// Unique name identifying this pass.
    fn name(&self) -> &str;

    /// Optional description of what this pass does.
    fn description(&self) -> &str {
        ""
    }

    /// Transform a query, returning the optimized version.
    /// Returns the query unchanged if no optimization applies.
    fn transform(&self, query: Query) -> Result<Query>;
}

/// Manages and executes a pipeline of optimization passes.
#[derive(Default)]
pub struct Optimizer {
    passes: Vec<Box<dyn OptimizationPass>>,
}

impl Optimizer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an optimizer with the default set of passes.
    pub fn with_defaults() -> Self {
        let mut opt = Self::new();
        opt.add_pass(Box::new(RemoveRedundantNesting));
        opt
    }

    /// Add an optimization pass to the pipeline.
    pub fn add_pass(&mut self, pass: Box<dyn OptimizationPass>) {
        self.passes.push(pass);
    }

    /// Run all optimization passes on a query in order.
    pub fn optimize(&self, query: Query) -> Result<Query> {
        let mut current = query;
        for pass in &self.passes {
            current = pass.transform(current)?;
        }
        Ok(current)
    }

    /// List registered pass names.
    pub fn pass_names(&self) -> Vec<&str> {
        self.passes.iter().map(|p| p.name()).collect()
    }
}

/// Built-in pass: removes unnecessary nested/parenthesized expressions.
///
/// Transforms `((x))` → `x` where the nesting doesn't affect semantics.
struct RemoveRedundantNesting;

impl OptimizationPass for RemoveRedundantNesting {
    fn name(&self) -> &str {
        "remove_redundant_nesting"
    }

    fn description(&self) -> &str {
        "Removes unnecessary parenthesized expressions"
    }

    fn transform(&self, query: Query) -> Result<Query> {
        match query {
            Query::Select(s) => Ok(Query::Select(Box::new(simplify_select(*s)))),
            other => Ok(other),
        }
    }
}

fn simplify_select(mut select: SelectQuery) -> SelectQuery {
    select.filter = select.filter.map(simplify_expr);
    select.having = select.having.map(simplify_expr);
    select.projections = select
        .projections
        .into_iter()
        .map(|item| match item {
            SelectItem::Expression { expr, alias } => SelectItem::Expression {
                expr: simplify_expr(expr),
                alias,
            },
            other => other,
        })
        .collect();
    select.group_by = select.group_by.into_iter().map(simplify_expr).collect();
    select.order_by = select
        .order_by
        .into_iter()
        .map(|o| OrderByExpr {
            expr: simplify_expr(o.expr),
            ..o
        })
        .collect();
    select
}

fn simplify_expr(expr: Expression) -> Expression {
    match expr {
        Expression::Nested(inner) => match *inner {
            // Remove double nesting: ((x)) → x
            Expression::Nested(_) => simplify_expr(*inner),
            // Remove nesting around simple expressions
            Expression::Column { .. }
            | Expression::Literal(_)
            | Expression::Wildcard
            | Expression::Parameter(_) => simplify_expr(*inner),
            // Keep nesting for complex expressions (may be needed for precedence)
            other => Expression::Nested(Box::new(simplify_expr(other))),
        },
        Expression::BinaryOp { left, op, right } => Expression::BinaryOp {
            left: Box::new(simplify_expr(*left)),
            op,
            right: Box::new(simplify_expr(*right)),
        },
        Expression::UnaryOp { op, expr } => Expression::UnaryOp {
            op,
            expr: Box::new(simplify_expr(*expr)),
        },
        Expression::Function {
            name,
            args,
            distinct,
        } => Expression::Function {
            name,
            args: args.into_iter().map(simplify_expr).collect(),
            distinct,
        },
        other => other,
    }
}

/// Analyze a query and return metadata about its structure.
pub fn analyze_query(query: &Query) -> QueryAnalysis {
    let mut analysis = QueryAnalysis::default();
    analyze_query_inner(query, &mut analysis);
    analysis
}

fn analyze_query_inner(query: &Query, analysis: &mut QueryAnalysis) {
    match query {
        Query::Select(s) => {
            analysis.has_select = true;
            if s.distinct {
                analysis.has_distinct = true;
            }
            if !s.joins.is_empty() {
                analysis.has_joins = true;
                analysis.join_count += s.joins.len();
            }
            if !s.group_by.is_empty() {
                analysis.has_aggregation = true;
            }
            if s.set_op.is_some() {
                analysis.has_set_operations = true;
            }
            if !s.windows.is_empty() {
                analysis.has_window_functions = true;
            }
            // Check projections for window functions
            for item in &s.projections {
                if let SelectItem::Expression { expr, .. } = item {
                    check_expr_features(expr, analysis);
                }
            }
            if let Some(ref filter) = s.filter {
                check_expr_features(filter, analysis);
            }
        }
        Query::With(cte) => {
            analysis.has_cte = true;
            if cte.recursive {
                analysis.has_recursive_cte = true;
            }
            for c in &cte.ctes {
                analyze_query_inner(&c.query, analysis);
            }
            analyze_query_inner(&cte.body, analysis);
        }
        Query::Insert(_) => analysis.has_insert = true,
        Query::Update(_) => analysis.has_update = true,
        Query::Delete(_) => analysis.has_delete = true,
        Query::Raw(_) => {}
    }
}

fn check_expr_features(expr: &Expression, analysis: &mut QueryAnalysis) {
    match expr {
        Expression::WindowFunction { .. } => analysis.has_window_functions = true,
        Expression::Subquery(q) | Expression::Exists(q) => {
            analysis.has_subqueries = true;
            analyze_query_inner(q, analysis);
        }
        Expression::InSubquery { subquery, .. } => {
            analysis.has_subqueries = true;
            analyze_query_inner(subquery, analysis);
        }
        Expression::Aggregate { .. } => analysis.has_aggregation = true,
        Expression::JsonAccess { .. } => analysis.has_json_operations = true,
        Expression::BinaryOp { left, right, .. } => {
            check_expr_features(left, analysis);
            check_expr_features(right, analysis);
        }
        Expression::UnaryOp { expr, .. } => check_expr_features(expr, analysis),
        Expression::Function { args, .. } => {
            for arg in args {
                check_expr_features(arg, analysis);
            }
        }
        Expression::Case {
            when_clauses,
            else_clause,
            ..
        } => {
            for (w, t) in when_clauses {
                check_expr_features(w, analysis);
                check_expr_features(t, analysis);
            }
            if let Some(e) = else_clause {
                check_expr_features(e, analysis);
            }
        }
        _ => {}
    }
}

/// Structural metadata about a query.
#[derive(Debug, Default, Clone)]
pub struct QueryAnalysis {
    pub has_select: bool,
    pub has_insert: bool,
    pub has_update: bool,
    pub has_delete: bool,
    pub has_distinct: bool,
    pub has_joins: bool,
    pub join_count: usize,
    pub has_aggregation: bool,
    pub has_window_functions: bool,
    pub has_subqueries: bool,
    pub has_cte: bool,
    pub has_recursive_cte: bool,
    pub has_set_operations: bool,
    pub has_json_operations: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::parser::parse_single;

    #[test]
    fn test_optimizer_empty() {
        let opt = Optimizer::new();
        let query = Query::Select(Box::new(SelectQuery::default()));
        let result = opt.optimize(query.clone()).unwrap();
        assert_eq!(result, query);
    }

    #[test]
    fn test_optimizer_with_defaults() {
        let opt = Optimizer::with_defaults();
        assert!(!opt.pass_names().is_empty());
        assert!(opt.pass_names().contains(&"remove_redundant_nesting"));
    }

    #[test]
    fn test_remove_redundant_nesting() {
        let pass = RemoveRedundantNesting;
        let query = Query::Select(Box::new(SelectQuery {
            filter: Some(Expression::Nested(Box::new(Expression::Column {
                table: None,
                name: "x".into(),
            }))),
            ..Default::default()
        }));

        let optimized = pass.transform(query).unwrap();
        match optimized {
            Query::Select(s) => {
                // The nesting around a simple column should be removed
                assert!(matches!(s.filter, Some(Expression::Column { .. })));
            }
            _ => panic!("Expected Select"),
        }
    }

    #[test]
    fn test_analyze_simple_select() {
        let q = parse_single("SELECT * FROM users").unwrap();
        let analysis = analyze_query(&q);
        assert!(analysis.has_select);
        assert!(!analysis.has_joins);
        assert!(!analysis.has_aggregation);
    }

    #[test]
    fn test_analyze_join_query() {
        let q =
            parse_single("SELECT * FROM a JOIN b ON a.id = b.a_id LEFT JOIN c ON b.id = c.b_id")
                .unwrap();
        let analysis = analyze_query(&q);
        assert!(analysis.has_joins);
        assert_eq!(analysis.join_count, 2);
    }

    #[test]
    fn test_analyze_aggregation() {
        let q = parse_single("SELECT dept, COUNT(*) FROM emp GROUP BY dept").unwrap();
        let analysis = analyze_query(&q);
        assert!(analysis.has_aggregation);
    }

    #[test]
    fn test_analyze_window_function() {
        let q = parse_single("SELECT ROW_NUMBER() OVER (PARTITION BY dept ORDER BY id) FROM emp")
            .unwrap();
        let analysis = analyze_query(&q);
        assert!(analysis.has_window_functions);
    }

    #[test]
    fn test_analyze_cte() {
        let q = parse_single("WITH cte AS (SELECT 1) SELECT * FROM cte").unwrap();
        let analysis = analyze_query(&q);
        assert!(analysis.has_cte);
        assert!(!analysis.has_recursive_cte);
    }

    #[test]
    fn test_analyze_recursive_cte() {
        let q = parse_single(
            "WITH RECURSIVE nums AS (SELECT 1 AS n UNION ALL SELECT n + 1 FROM nums WHERE n < 10) SELECT * FROM nums",
        )
        .unwrap();
        let analysis = analyze_query(&q);
        assert!(analysis.has_cte);
        assert!(analysis.has_recursive_cte);
    }

    #[test]
    fn test_analyze_subquery() {
        let q =
            parse_single("SELECT * FROM users WHERE id IN (SELECT user_id FROM active)").unwrap();
        let analysis = analyze_query(&q);
        assert!(analysis.has_subqueries);
    }

    #[test]
    fn test_analyze_set_operation() {
        let q = parse_single("SELECT id FROM a UNION SELECT id FROM b").unwrap();
        let analysis = analyze_query(&q);
        assert!(analysis.has_set_operations);
    }
}
