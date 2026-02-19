//! Unified Query AST types for pgrsql.
//!
//! This module defines the internal representation used by all language adapters,
//! optimization passes, and the SQL compiler. The AST is designed to be:
//! - Language-agnostic (any DSL can compile to it)
//! - Immutable-friendly (clone-based transformations)
//! - Extensible (new node types can be added without breaking existing passes)

/// Top-level query representation.
#[derive(Debug, Clone, PartialEq)]
pub enum Query {
    Select(Box<SelectQuery>),
    Insert(InsertQuery),
    Update(UpdateQuery),
    Delete(DeleteQuery),
    /// Common Table Expressions wrapping an inner query.
    With(CTEQuery),
    /// Raw SQL passthrough for unsupported or complex statements.
    Raw(String),
}

/// A SELECT query with all standard SQL clauses.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SelectQuery {
    pub distinct: bool,
    pub projections: Vec<SelectItem>,
    pub from: Vec<TableRef>,
    pub joins: Vec<Join>,
    pub filter: Option<Expression>,
    pub group_by: Vec<Expression>,
    pub having: Option<Expression>,
    pub windows: Vec<NamedWindowSpec>,
    pub order_by: Vec<OrderByExpr>,
    pub limit: Option<Expression>,
    pub offset: Option<Expression>,
    /// Set operations (UNION, INTERSECT, EXCEPT).
    pub set_op: Option<Box<SetOperation>>,
}

/// A single item in the SELECT projection list.
#[derive(Debug, Clone, PartialEq)]
pub enum SelectItem {
    /// `*`
    Wildcard,
    /// `table.*`
    QualifiedWildcard(String),
    /// An expression, optionally aliased: `expr AS alias`.
    Expression {
        expr: Expression,
        alias: Option<String>,
    },
}

/// Table reference in FROM clause.
#[derive(Debug, Clone, PartialEq)]
pub enum TableRef {
    /// Simple table: `schema.table AS alias`
    Table {
        schema: Option<String>,
        name: String,
        alias: Option<String>,
    },
    /// Subquery: `(SELECT ...) AS alias`
    Subquery { query: Box<Query>, alias: String },
    /// Table-valued function: `generate_series(1, 10) AS alias`
    Function {
        name: String,
        args: Vec<Expression>,
        alias: Option<String>,
    },
}

/// JOIN clause representation.
#[derive(Debug, Clone, PartialEq)]
pub struct Join {
    pub join_type: JoinType,
    pub table: TableRef,
    pub condition: Option<JoinCondition>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum JoinType {
    Inner,
    Left,
    Right,
    Full,
    Cross,
    Lateral,
}

#[derive(Debug, Clone, PartialEq)]
pub enum JoinCondition {
    On(Expression),
    Using(Vec<String>),
    Natural,
}

/// Core expression type. Recursive to support arbitrary nesting.
#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    /// Column reference: `table.column` or just `column`.
    Column { table: Option<String>, name: String },
    /// Literal value.
    Literal(Literal),
    /// Binary operation: `left op right`.
    BinaryOp {
        left: Box<Expression>,
        op: BinaryOperator,
        right: Box<Expression>,
    },
    /// Unary operation: `op expr` (e.g., NOT, -).
    UnaryOp {
        op: UnaryOperator,
        expr: Box<Expression>,
    },
    /// Function call: `name(args)`.
    Function {
        name: String,
        args: Vec<Expression>,
        distinct: bool,
    },
    /// Aggregate function with optional filter.
    Aggregate {
        name: String,
        args: Vec<Expression>,
        distinct: bool,
        filter: Option<Box<Expression>>,
    },
    /// Window function: `expr OVER (...)`.
    WindowFunction {
        function: Box<Expression>,
        window: WindowSpec,
    },
    /// CASE expression.
    Case {
        operand: Option<Box<Expression>>,
        when_clauses: Vec<(Expression, Expression)>,
        else_clause: Option<Box<Expression>>,
    },
    /// Subquery expression: `(SELECT ...)`.
    Subquery(Box<Query>),
    /// EXISTS (SELECT ...).
    Exists(Box<Query>),
    /// expr IN (values or subquery).
    InList {
        expr: Box<Expression>,
        list: Vec<Expression>,
        negated: bool,
    },
    InSubquery {
        expr: Box<Expression>,
        subquery: Box<Query>,
        negated: bool,
    },
    /// expr BETWEEN low AND high.
    Between {
        expr: Box<Expression>,
        low: Box<Expression>,
        high: Box<Expression>,
        negated: bool,
    },
    /// expr IS NULL / IS NOT NULL.
    IsNull {
        expr: Box<Expression>,
        negated: bool,
    },
    /// CAST(expr AS type).
    Cast {
        expr: Box<Expression>,
        data_type: String,
    },
    /// Wildcard `*` (used in COUNT(*)).
    Wildcard,
    /// Parameter placeholder: `$1`, `$2`, etc.
    Parameter(usize),
    /// Array expression: `ARRAY[...]`.
    Array(Vec<Expression>),
    /// JSON access: `expr->key`, `expr->>key`.
    JsonAccess {
        expr: Box<Expression>,
        path: Box<Expression>,
        as_text: bool,
    },
    /// Type-cast using `::` operator (PostgreSQL specific).
    TypeCast {
        expr: Box<Expression>,
        data_type: String,
    },
    /// Nested expression (parenthesized).
    Nested(Box<Expression>),
}

/// Literal values in SQL.
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Null,
    Boolean(bool),
    Integer(i64),
    Float(f64),
    String(String),
}

/// Binary operators.
#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOperator {
    // Comparison
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    // Logical
    And,
    Or,
    // Arithmetic
    Plus,
    Minus,
    Multiply,
    Divide,
    Modulo,
    // String
    Like,
    ILike,
    NotLike,
    NotILike,
    // Other
    Concat,
}

/// Unary operators.
#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOperator {
    Not,
    Minus,
    Plus,
}

/// Window specification for window functions.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowSpec {
    pub partition_by: Vec<Expression>,
    pub order_by: Vec<OrderByExpr>,
    pub frame: Option<WindowFrame>,
}

/// Named window definition for WINDOW clause.
#[derive(Debug, Clone, PartialEq)]
pub struct NamedWindowSpec {
    pub name: String,
    pub spec: WindowSpec,
}

/// Window frame specification.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowFrame {
    pub mode: WindowFrameMode,
    pub start: WindowFrameBound,
    pub end: Option<WindowFrameBound>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WindowFrameMode {
    Rows,
    Range,
    Groups,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WindowFrameBound {
    CurrentRow,
    Preceding(Option<u64>),
    Following(Option<u64>),
}

/// ORDER BY expression.
#[derive(Debug, Clone, PartialEq)]
pub struct OrderByExpr {
    pub expr: Expression,
    pub asc: Option<bool>,
    pub nulls_first: Option<bool>,
}

/// Set operations (UNION, INTERSECT, EXCEPT).
#[derive(Debug, Clone, PartialEq)]
pub struct SetOperation {
    pub op: SetOperator,
    pub all: bool,
    pub right: Query,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SetOperator {
    Union,
    Intersect,
    Except,
}

/// Common Table Expression (WITH clause).
#[derive(Debug, Clone, PartialEq)]
pub struct CTEQuery {
    pub recursive: bool,
    pub ctes: Vec<CTE>,
    pub body: Box<Query>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CTE {
    pub name: String,
    pub columns: Vec<String>,
    pub query: Query,
}

/// INSERT statement.
#[derive(Debug, Clone, PartialEq)]
pub struct InsertQuery {
    pub table: TableRef,
    pub columns: Vec<String>,
    pub source: InsertSource,
    pub returning: Vec<SelectItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InsertSource {
    Values(Vec<Vec<Expression>>),
    Query(Box<Query>),
}

/// UPDATE statement.
#[derive(Debug, Clone, PartialEq)]
pub struct UpdateQuery {
    pub table: TableRef,
    pub assignments: Vec<Assignment>,
    pub filter: Option<Expression>,
    pub returning: Vec<SelectItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    pub column: String,
    pub value: Expression,
}

/// DELETE statement.
#[derive(Debug, Clone, PartialEq)]
pub struct DeleteQuery {
    pub table: TableRef,
    pub filter: Option<Expression>,
    pub returning: Vec<SelectItem>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_select_query() {
        let q = SelectQuery::default();
        assert!(!q.distinct);
        assert!(q.projections.is_empty());
        assert!(q.from.is_empty());
        assert!(q.filter.is_none());
        assert!(q.limit.is_none());
    }

    #[test]
    fn test_query_clone() {
        let q = Query::Select(Box::new(SelectQuery {
            distinct: true,
            projections: vec![SelectItem::Wildcard],
            from: vec![TableRef::Table {
                schema: None,
                name: "users".into(),
                alias: None,
            }],
            ..Default::default()
        }));
        let q2 = q.clone();
        assert_eq!(q, q2);
    }

    #[test]
    fn test_expression_nesting() {
        let expr = Expression::BinaryOp {
            left: Box::new(Expression::Column {
                table: None,
                name: "age".into(),
            }),
            op: BinaryOperator::Gt,
            right: Box::new(Expression::Literal(Literal::Integer(18))),
        };
        // Verify we can clone deeply nested expressions
        let _ = expr.clone();
    }

    #[test]
    fn test_literal_equality() {
        assert_eq!(Literal::Null, Literal::Null);
        assert_eq!(Literal::Boolean(true), Literal::Boolean(true));
        assert_ne!(Literal::Integer(1), Literal::Integer(2));
        assert_eq!(
            Literal::String("hello".into()),
            Literal::String("hello".into())
        );
    }

    #[test]
    fn test_cte_query_structure() {
        let cte = CTEQuery {
            recursive: true,
            ctes: vec![CTE {
                name: "recursive_cte".into(),
                columns: vec!["n".into()],
                query: Query::Select(Box::new(SelectQuery {
                    projections: vec![SelectItem::Expression {
                        expr: Expression::Literal(Literal::Integer(1)),
                        alias: Some("n".into()),
                    }],
                    ..Default::default()
                })),
            }],
            body: Box::new(Query::Select(Box::new(SelectQuery::default()))),
        };
        assert!(cte.recursive);
        assert_eq!(cte.ctes.len(), 1);
        assert_eq!(cte.ctes[0].name, "recursive_cte");
    }
}
