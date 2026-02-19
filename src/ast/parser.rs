/// SQL → Unified AST parser.
///
/// Translates SQL text into our internal AST representation using `sqlparser`
/// as the parsing frontend. This decouples our AST from the sqlparser crate,
/// allowing us to evolve our representation independently.
use anyhow::{anyhow, Result};
use sqlparser::ast as sp;
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser as SqlParser;

use super::types::*;

/// Parse a SQL string into our unified AST.
pub fn parse_sql(sql: &str) -> Result<Vec<Query>> {
    let dialect = PostgreSqlDialect {};
    let statements =
        SqlParser::parse_sql(&dialect, sql).map_err(|e| anyhow!("SQL parse error: {}", e))?;

    statements.into_iter().map(convert_statement).collect()
}

/// Parse a single SQL statement. Returns an error if the input contains
/// more than one statement.
pub fn parse_single(sql: &str) -> Result<Query> {
    let mut queries = parse_sql(sql)?;
    if queries.len() != 1 {
        return Err(anyhow!("Expected 1 statement, found {}", queries.len()));
    }
    Ok(queries.remove(0))
}

fn convert_statement(stmt: sp::Statement) -> Result<Query> {
    match stmt {
        sp::Statement::Query(q) => convert_query(*q),
        sp::Statement::Insert(insert) => convert_insert(insert),
        sp::Statement::Update {
            table,
            assignments,
            selection,
            returning,
            ..
        } => convert_update(table, assignments, selection, returning),
        sp::Statement::Delete(delete) => convert_delete(delete),
        _ => Ok(Query::Raw(stmt.to_string())),
    }
}

fn convert_query(query: sp::Query) -> Result<Query> {
    // Extract order_by exprs from Option<OrderBy>
    let order_by_exprs: Vec<sp::OrderByExpr> =
        query.order_by.map(|ob| ob.exprs).unwrap_or_default();

    // Handle CTEs
    if let Some(with) = query.with {
        let recursive = with.recursive;
        let ctes = with
            .cte_tables
            .into_iter()
            .map(convert_cte)
            .collect::<Result<Vec<_>>>()?;

        let body = convert_set_expr(*query.body)?;

        // Apply ORDER BY, LIMIT, OFFSET to the body
        let body = apply_query_modifiers(body, &order_by_exprs, &query.limit, &query.offset)?;

        return Ok(Query::With(CTEQuery {
            recursive,
            ctes,
            body: Box::new(body),
        }));
    }

    let body = convert_set_expr(*query.body)?;
    apply_query_modifiers(body, &order_by_exprs, &query.limit, &query.offset)
}

fn apply_query_modifiers(
    query: Query,
    order_by: &[sp::OrderByExpr],
    limit: &Option<sp::Expr>,
    offset: &Option<sp::Offset>,
) -> Result<Query> {
    // Only apply modifiers to Select queries
    if let Query::Select(mut select) = query {
        if !order_by.is_empty() {
            select.order_by = order_by
                .iter()
                .map(|o| convert_order_by(o.clone()))
                .collect::<Result<Vec<_>>>()?;
        }
        if let Some(l) = limit {
            select.limit = Some(convert_expr(l.clone())?);
        }
        if let Some(o) = offset {
            select.offset = Some(convert_expr(o.value.clone())?);
        }
        Ok(Query::Select(select))
    } else {
        Ok(query)
    }
}

fn convert_cte(cte: sp::Cte) -> Result<CTE> {
    let columns = match cte.alias.columns.is_empty() {
        true => vec![],
        false => cte
            .alias
            .columns
            .iter()
            .map(|c| c.name.value.clone())
            .collect(),
    };
    Ok(CTE {
        name: cte.alias.name.value.clone(),
        columns,
        query: convert_query(*cte.query)?,
    })
}

fn convert_set_expr(expr: sp::SetExpr) -> Result<Query> {
    match expr {
        sp::SetExpr::Select(select) => convert_select(*select),
        sp::SetExpr::Query(query) => convert_query(*query),
        sp::SetExpr::SetOperation {
            op,
            set_quantifier,
            left,
            right,
            ..
        } => {
            let left_query = convert_set_expr(*left)?;
            let right_query = convert_set_expr(*right)?;

            let all = matches!(
                set_quantifier,
                sp::SetQuantifier::All | sp::SetQuantifier::AllByName
            );

            let set_op = SetOperation {
                op: match op {
                    sp::SetOperator::Union => SetOperator::Union,
                    sp::SetOperator::Intersect => SetOperator::Intersect,
                    sp::SetOperator::Except => SetOperator::Except,
                },
                all,
                right: right_query,
            };

            match left_query {
                Query::Select(mut s) => {
                    s.set_op = Some(Box::new(set_op));
                    Ok(Query::Select(s))
                }
                other => {
                    // Wrap in a basic select
                    let s = SelectQuery {
                        set_op: Some(Box::new(set_op)),
                        from: vec![TableRef::Subquery {
                            query: Box::new(other),
                            alias: "_left".into(),
                        }],
                        ..Default::default()
                    };
                    Ok(Query::Select(Box::new(s)))
                }
            }
        }
        sp::SetExpr::Values(values) => {
            // VALUES as a standalone query - wrap in raw
            Ok(Query::Raw(format!("VALUES {}", values)))
        }
        _ => Ok(Query::Raw(expr.to_string())),
    }
}

fn convert_select(select: sp::Select) -> Result<Query> {
    let distinct = select.distinct.is_some();

    let projections = select
        .projection
        .into_iter()
        .map(convert_select_item)
        .collect::<Result<Vec<_>>>()?;

    let from = select
        .from
        .into_iter()
        .map(convert_table_with_joins)
        .collect::<Result<Vec<_>>>()?;

    // Flatten: first element is the table, rest are joins
    let (tables, join_lists): (Vec<_>, Vec<_>) = from.into_iter().unzip();

    let joins: Vec<Join> = join_lists.into_iter().flatten().collect();

    let filter = select.selection.map(convert_expr).transpose()?;

    let group_by = match select.group_by {
        sp::GroupByExpr::Expressions(exprs, _modifiers) => exprs
            .into_iter()
            .map(convert_expr)
            .collect::<Result<Vec<_>>>()?,
        sp::GroupByExpr::All(_) => vec![],
    };

    let having = select.having.map(convert_expr).transpose()?;

    let windows = select
        .named_window
        .into_iter()
        .map(|nw| {
            let spec = convert_window_spec_from_named(&nw.1);
            Ok(NamedWindowSpec {
                name: nw.0.value.clone(),
                spec: spec?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(Query::Select(Box::new(SelectQuery {
        distinct,
        projections,
        from: tables,
        joins,
        filter,
        group_by,
        having,
        windows,
        order_by: vec![],
        limit: None,
        offset: None,
        set_op: None,
    })))
}

fn convert_table_with_joins(twj: sp::TableWithJoins) -> Result<(TableRef, Vec<Join>)> {
    let table = convert_table_factor(twj.relation)?;
    let joins = twj
        .joins
        .into_iter()
        .map(convert_join)
        .collect::<Result<Vec<_>>>()?;
    Ok((table, joins))
}

fn convert_table_factor(tf: sp::TableFactor) -> Result<TableRef> {
    match tf {
        sp::TableFactor::Table { name, alias, .. } => {
            let parts: Vec<&str> = name.0.iter().map(|p| p.value.as_str()).collect();
            let (schema, table_name) = match parts.len() {
                1 => (None, parts[0].to_string()),
                2 => (Some(parts[0].to_string()), parts[1].to_string()),
                _ => (None, name.to_string()),
            };
            Ok(TableRef::Table {
                schema,
                name: table_name,
                alias: alias.map(|a| a.name.value),
            })
        }
        sp::TableFactor::Derived {
            subquery, alias, ..
        } => {
            let alias_name = alias
                .map(|a| a.name.value)
                .unwrap_or_else(|| "_subquery".into());
            Ok(TableRef::Subquery {
                query: Box::new(convert_query(*subquery)?),
                alias: alias_name,
            })
        }
        sp::TableFactor::TableFunction { expr, alias } => Ok(TableRef::Function {
            name: expr.to_string(),
            args: vec![],
            alias: alias.map(|a| a.name.value),
        }),
        _ => Ok(TableRef::Table {
            schema: None,
            name: tf.to_string(),
            alias: None,
        }),
    }
}

fn convert_join(join: sp::Join) -> Result<Join> {
    let join_type = match &join.join_operator {
        sp::JoinOperator::Inner(_) => JoinType::Inner,
        sp::JoinOperator::LeftOuter(_) => JoinType::Left,
        sp::JoinOperator::RightOuter(_) => JoinType::Right,
        sp::JoinOperator::FullOuter(_) => JoinType::Full,
        sp::JoinOperator::CrossJoin => JoinType::Cross,
        _ => JoinType::Inner,
    };

    let condition = match &join.join_operator {
        sp::JoinOperator::Inner(c)
        | sp::JoinOperator::LeftOuter(c)
        | sp::JoinOperator::RightOuter(c)
        | sp::JoinOperator::FullOuter(c) => convert_join_constraint(c)?,
        _ => None,
    };

    Ok(Join {
        join_type,
        table: convert_table_factor(join.relation)?,
        condition,
    })
}

fn convert_join_constraint(constraint: &sp::JoinConstraint) -> Result<Option<JoinCondition>> {
    match constraint {
        sp::JoinConstraint::On(expr) => Ok(Some(JoinCondition::On(convert_expr(expr.clone())?))),
        sp::JoinConstraint::Using(cols) => Ok(Some(JoinCondition::Using(
            cols.iter().map(|c| c.value.clone()).collect(),
        ))),
        sp::JoinConstraint::Natural => Ok(Some(JoinCondition::Natural)),
        sp::JoinConstraint::None => Ok(None),
    }
}

fn convert_select_item(item: sp::SelectItem) -> Result<SelectItem> {
    match item {
        sp::SelectItem::UnnamedExpr(expr) => Ok(SelectItem::Expression {
            expr: convert_expr(expr)?,
            alias: None,
        }),
        sp::SelectItem::ExprWithAlias { expr, alias } => Ok(SelectItem::Expression {
            expr: convert_expr(expr)?,
            alias: Some(alias.value),
        }),
        sp::SelectItem::Wildcard(_) => Ok(SelectItem::Wildcard),
        sp::SelectItem::QualifiedWildcard(name, _) => {
            Ok(SelectItem::QualifiedWildcard(name.to_string()))
        }
    }
}

fn convert_expr(expr: sp::Expr) -> Result<Expression> {
    match expr {
        sp::Expr::Identifier(ident) => Ok(Expression::Column {
            table: None,
            name: ident.value,
        }),
        sp::Expr::CompoundIdentifier(parts) => {
            let names: Vec<String> = parts.into_iter().map(|p| p.value).collect();
            match names.len() {
                1 => Ok(Expression::Column {
                    table: None,
                    name: names.into_iter().next().unwrap(),
                }),
                2 => {
                    let mut iter = names.into_iter();
                    Ok(Expression::Column {
                        table: Some(iter.next().unwrap()),
                        name: iter.next().unwrap(),
                    })
                }
                _ => Ok(Expression::Column {
                    table: None,
                    name: names.join("."),
                }),
            }
        }
        sp::Expr::Value(val) => convert_value(val),
        sp::Expr::BinaryOp { left, op, right } => Ok(Expression::BinaryOp {
            left: Box::new(convert_expr(*left)?),
            op: convert_binary_op(op)?,
            right: Box::new(convert_expr(*right)?),
        }),
        sp::Expr::UnaryOp { op, expr } => Ok(Expression::UnaryOp {
            op: convert_unary_op(op)?,
            expr: Box::new(convert_expr(*expr)?),
        }),
        sp::Expr::Function(func) => convert_function(func),
        sp::Expr::Case {
            operand,
            conditions,
            results,
            else_result,
        } => {
            let when_clauses = conditions
                .into_iter()
                .zip(results)
                .map(|(c, r)| Ok((convert_expr(c)?, convert_expr(r)?)))
                .collect::<Result<Vec<_>>>()?;
            Ok(Expression::Case {
                operand: operand.map(|o| convert_expr(*o)).transpose()?.map(Box::new),
                when_clauses,
                else_clause: else_result
                    .map(|e| convert_expr(*e))
                    .transpose()?
                    .map(Box::new),
            })
        }
        sp::Expr::Subquery(q) => Ok(Expression::Subquery(Box::new(convert_query(*q)?))),
        sp::Expr::Exists { subquery, negated } => {
            let exists = Expression::Exists(Box::new(convert_query(*subquery)?));
            if negated {
                Ok(Expression::UnaryOp {
                    op: UnaryOperator::Not,
                    expr: Box::new(exists),
                })
            } else {
                Ok(exists)
            }
        }
        sp::Expr::InList {
            expr,
            list,
            negated,
        } => Ok(Expression::InList {
            expr: Box::new(convert_expr(*expr)?),
            list: list
                .into_iter()
                .map(convert_expr)
                .collect::<Result<Vec<_>>>()?,
            negated,
        }),
        sp::Expr::InSubquery {
            expr,
            subquery,
            negated,
        } => Ok(Expression::InSubquery {
            expr: Box::new(convert_expr(*expr)?),
            subquery: Box::new(convert_query(*subquery)?),
            negated,
        }),
        sp::Expr::Between {
            expr,
            negated,
            low,
            high,
        } => Ok(Expression::Between {
            expr: Box::new(convert_expr(*expr)?),
            low: Box::new(convert_expr(*low)?),
            high: Box::new(convert_expr(*high)?),
            negated,
        }),
        sp::Expr::IsNull(expr) => Ok(Expression::IsNull {
            expr: Box::new(convert_expr(*expr)?),
            negated: false,
        }),
        sp::Expr::IsNotNull(expr) => Ok(Expression::IsNull {
            expr: Box::new(convert_expr(*expr)?),
            negated: true,
        }),
        sp::Expr::Cast {
            expr, data_type, ..
        } => Ok(Expression::Cast {
            expr: Box::new(convert_expr(*expr)?),
            data_type: data_type.to_string(),
        }),
        sp::Expr::Nested(expr) => Ok(Expression::Nested(Box::new(convert_expr(*expr)?))),
        sp::Expr::Like {
            negated,
            expr,
            pattern,
            ..
        } => {
            let op = if negated {
                BinaryOperator::NotLike
            } else {
                BinaryOperator::Like
            };
            Ok(Expression::BinaryOp {
                left: Box::new(convert_expr(*expr)?),
                op,
                right: Box::new(convert_expr(*pattern)?),
            })
        }
        sp::Expr::ILike {
            negated,
            expr,
            pattern,
            ..
        } => {
            let op = if negated {
                BinaryOperator::NotILike
            } else {
                BinaryOperator::ILike
            };
            Ok(Expression::BinaryOp {
                left: Box::new(convert_expr(*expr)?),
                op,
                right: Box::new(convert_expr(*pattern)?),
            })
        }
        sp::Expr::Array(arr) => {
            let elems = arr
                .elem
                .into_iter()
                .map(convert_expr)
                .collect::<Result<Vec<_>>>()?;
            Ok(Expression::Array(elems))
        }
        sp::Expr::JsonAccess { value, path } => convert_json_access(*value, path),
        _ => {
            // Fallback: store as a literal string representation
            Ok(Expression::Literal(Literal::String(expr.to_string())))
        }
    }
}

fn convert_json_access(value: sp::Expr, path: sp::JsonPath) -> Result<Expression> {
    let base = convert_expr(value)?;
    let mut current = base;

    for element in path.path {
        match element {
            sp::JsonPathElem::Dot { key, .. } => {
                current = Expression::JsonAccess {
                    expr: Box::new(current),
                    path: Box::new(Expression::Literal(Literal::String(key))),
                    as_text: false,
                };
            }
            sp::JsonPathElem::Bracket { key } => {
                current = Expression::JsonAccess {
                    expr: Box::new(current),
                    path: Box::new(convert_expr(key)?),
                    as_text: false,
                };
            }
        }
    }

    Ok(current)
}

fn convert_value(val: sp::Value) -> Result<Expression> {
    match val {
        sp::Value::Null => Ok(Expression::Literal(Literal::Null)),
        sp::Value::Boolean(b) => Ok(Expression::Literal(Literal::Boolean(b))),
        sp::Value::Number(n, _) => {
            if let Ok(i) = n.parse::<i64>() {
                Ok(Expression::Literal(Literal::Integer(i)))
            } else if let Ok(f) = n.parse::<f64>() {
                Ok(Expression::Literal(Literal::Float(f)))
            } else {
                Ok(Expression::Literal(Literal::String(n)))
            }
        }
        sp::Value::SingleQuotedString(s) => Ok(Expression::Literal(Literal::String(s))),
        sp::Value::DoubleQuotedString(s) => Ok(Expression::Literal(Literal::String(s))),
        sp::Value::Placeholder(p) => {
            // Parse $1, $2, etc.
            if let Some(n) = p.strip_prefix('$') {
                if let Ok(idx) = n.parse::<usize>() {
                    return Ok(Expression::Parameter(idx));
                }
            }
            Ok(Expression::Literal(Literal::String(p)))
        }
        _ => Ok(Expression::Literal(Literal::String(val.to_string()))),
    }
}

fn convert_binary_op(op: sp::BinaryOperator) -> Result<BinaryOperator> {
    match op {
        sp::BinaryOperator::Eq => Ok(BinaryOperator::Eq),
        sp::BinaryOperator::NotEq => Ok(BinaryOperator::NotEq),
        sp::BinaryOperator::Lt => Ok(BinaryOperator::Lt),
        sp::BinaryOperator::LtEq => Ok(BinaryOperator::LtEq),
        sp::BinaryOperator::Gt => Ok(BinaryOperator::Gt),
        sp::BinaryOperator::GtEq => Ok(BinaryOperator::GtEq),
        sp::BinaryOperator::And => Ok(BinaryOperator::And),
        sp::BinaryOperator::Or => Ok(BinaryOperator::Or),
        sp::BinaryOperator::Plus => Ok(BinaryOperator::Plus),
        sp::BinaryOperator::Minus => Ok(BinaryOperator::Minus),
        sp::BinaryOperator::Multiply => Ok(BinaryOperator::Multiply),
        sp::BinaryOperator::Divide => Ok(BinaryOperator::Divide),
        sp::BinaryOperator::Modulo => Ok(BinaryOperator::Modulo),
        sp::BinaryOperator::StringConcat => Ok(BinaryOperator::Concat),
        _ => Err(anyhow!("Unsupported binary operator: {:?}", op)),
    }
}

fn convert_unary_op(op: sp::UnaryOperator) -> Result<UnaryOperator> {
    match op {
        sp::UnaryOperator::Not => Ok(UnaryOperator::Not),
        sp::UnaryOperator::Minus => Ok(UnaryOperator::Minus),
        sp::UnaryOperator::Plus => Ok(UnaryOperator::Plus),
        _ => Err(anyhow!("Unsupported unary operator: {:?}", op)),
    }
}

fn convert_function(func: sp::Function) -> Result<Expression> {
    let name = func.name.to_string().to_uppercase();

    let (args, distinct) = match func.args {
        sp::FunctionArguments::List(arg_list) => {
            let distinct = matches!(
                arg_list.duplicate_treatment,
                Some(sp::DuplicateTreatment::Distinct)
            );
            let args = arg_list
                .args
                .into_iter()
                .filter_map(|a| match a {
                    sp::FunctionArg::Unnamed(sp::FunctionArgExpr::Expr(e)) => Some(convert_expr(e)),
                    sp::FunctionArg::Unnamed(sp::FunctionArgExpr::Wildcard) => {
                        Some(Ok(Expression::Wildcard))
                    }
                    sp::FunctionArg::Named {
                        arg: sp::FunctionArgExpr::Expr(e),
                        ..
                    } => Some(convert_expr(e)),
                    _ => None,
                })
                .collect::<Result<Vec<_>>>()?;
            (args, distinct)
        }
        sp::FunctionArguments::None => (vec![], false),
        sp::FunctionArguments::Subquery(q) => (
            vec![Expression::Subquery(Box::new(convert_query(*q)?))],
            false,
        ),
    };

    // Check if this is a window function
    if let Some(over) = func.over {
        let window = match over {
            sp::WindowType::WindowSpec(spec) => convert_window_spec(spec)?,
            sp::WindowType::NamedWindow(_name) => {
                // Reference to a named window - use empty spec as placeholder
                WindowSpec {
                    partition_by: vec![],
                    order_by: vec![],
                    frame: None,
                }
            }
        };

        let function = Expression::Function {
            name,
            args,
            distinct,
        };

        return Ok(Expression::WindowFunction {
            function: Box::new(function),
            window,
        });
    }

    // Check if it's an aggregate function
    let is_aggregate = matches!(
        name.as_str(),
        "COUNT"
            | "SUM"
            | "AVG"
            | "MIN"
            | "MAX"
            | "ARRAY_AGG"
            | "STRING_AGG"
            | "BOOL_AND"
            | "BOOL_OR"
    );

    if is_aggregate {
        Ok(Expression::Aggregate {
            name,
            args,
            distinct,
            filter: None,
        })
    } else {
        Ok(Expression::Function {
            name,
            args,
            distinct,
        })
    }
}

fn convert_window_spec(spec: sp::WindowSpec) -> Result<WindowSpec> {
    let partition_by = spec
        .partition_by
        .into_iter()
        .map(convert_expr)
        .collect::<Result<Vec<_>>>()?;

    let order_by = spec
        .order_by
        .into_iter()
        .map(convert_order_by)
        .collect::<Result<Vec<_>>>()?;

    let frame = spec.window_frame.map(convert_window_frame).transpose()?;

    Ok(WindowSpec {
        partition_by,
        order_by,
        frame,
    })
}

fn convert_window_spec_from_named(spec: &sp::NamedWindowExpr) -> Result<WindowSpec> {
    match spec {
        sp::NamedWindowExpr::NamedWindow(_ident) => Ok(WindowSpec {
            partition_by: vec![],
            order_by: vec![],
            frame: None,
        }),
        sp::NamedWindowExpr::WindowSpec(spec) => convert_window_spec(spec.clone()),
    }
}

fn convert_window_frame(frame: sp::WindowFrame) -> Result<WindowFrame> {
    let mode = match frame.units {
        sp::WindowFrameUnits::Rows => WindowFrameMode::Rows,
        sp::WindowFrameUnits::Range => WindowFrameMode::Range,
        sp::WindowFrameUnits::Groups => WindowFrameMode::Groups,
    };

    let start = convert_window_frame_bound(frame.start_bound)?;
    let end = frame
        .end_bound
        .map(convert_window_frame_bound)
        .transpose()?;

    Ok(WindowFrame { mode, start, end })
}

fn convert_window_frame_bound(bound: sp::WindowFrameBound) -> Result<WindowFrameBound> {
    match bound {
        sp::WindowFrameBound::CurrentRow => Ok(WindowFrameBound::CurrentRow),
        sp::WindowFrameBound::Preceding(None) => Ok(WindowFrameBound::Preceding(None)),
        sp::WindowFrameBound::Preceding(Some(expr)) => {
            if let sp::Expr::Value(sp::Value::Number(n, _)) = *expr {
                Ok(WindowFrameBound::Preceding(Some(n.parse().unwrap_or(0))))
            } else {
                Ok(WindowFrameBound::Preceding(None))
            }
        }
        sp::WindowFrameBound::Following(None) => Ok(WindowFrameBound::Following(None)),
        sp::WindowFrameBound::Following(Some(expr)) => {
            if let sp::Expr::Value(sp::Value::Number(n, _)) = *expr {
                Ok(WindowFrameBound::Following(Some(n.parse().unwrap_or(0))))
            } else {
                Ok(WindowFrameBound::Following(None))
            }
        }
    }
}

fn convert_order_by(order: sp::OrderByExpr) -> Result<OrderByExpr> {
    Ok(OrderByExpr {
        expr: convert_expr(order.expr)?,
        asc: order.asc,
        nulls_first: order.nulls_first,
    })
}

fn convert_insert(insert: sp::Insert) -> Result<Query> {
    let table_name = insert.table_name.to_string();
    let table = TableRef::Table {
        schema: None,
        name: table_name,
        alias: None,
    };

    let columns: Vec<String> = insert.columns.iter().map(|c| c.value.clone()).collect();

    let source = if let Some(src) = insert.source {
        match *src.body {
            sp::SetExpr::Values(values) => {
                let rows = values
                    .rows
                    .into_iter()
                    .map(|row| {
                        row.into_iter()
                            .map(convert_expr)
                            .collect::<Result<Vec<_>>>()
                    })
                    .collect::<Result<Vec<_>>>()?;
                InsertSource::Values(rows)
            }
            other => {
                let query = convert_set_expr(other)?;
                InsertSource::Query(Box::new(query))
            }
        }
    } else {
        InsertSource::Values(vec![])
    };

    let returning = insert
        .returning
        .unwrap_or_default()
        .into_iter()
        .map(convert_select_item)
        .collect::<Result<Vec<_>>>()?;

    Ok(Query::Insert(InsertQuery {
        table,
        columns,
        source,
        returning,
    }))
}

fn convert_update(
    table: sp::TableWithJoins,
    assignments: Vec<sp::Assignment>,
    selection: Option<sp::Expr>,
    returning: Option<Vec<sp::SelectItem>>,
) -> Result<Query> {
    let table_ref = convert_table_factor(table.relation)?;

    let assigns = assignments
        .into_iter()
        .map(|a| {
            let column = a.target.to_string();
            Ok(Assignment {
                column,
                value: convert_expr(a.value)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let filter = selection.map(convert_expr).transpose()?;

    let ret = returning
        .unwrap_or_default()
        .into_iter()
        .map(convert_select_item)
        .collect::<Result<Vec<_>>>()?;

    Ok(Query::Update(UpdateQuery {
        table: table_ref,
        assignments: assigns,
        filter,
        returning: ret,
    }))
}

fn convert_delete(delete: sp::Delete) -> Result<Query> {
    // Extract tables from FromTable enum
    let from_tables = match delete.from {
        sp::FromTable::WithFromKeyword(tables) => tables,
        sp::FromTable::WithoutKeyword(tables) => tables,
    };

    let table_ref = if let Some(twj) = from_tables.into_iter().next() {
        convert_table_factor(twj.relation)?
    } else {
        return Err(anyhow!("DELETE without table reference"));
    };

    let filter = delete.selection.map(convert_expr).transpose()?;

    let returning = delete
        .returning
        .unwrap_or_default()
        .into_iter()
        .map(convert_select_item)
        .collect::<Result<Vec<_>>>()?;

    Ok(Query::Delete(DeleteQuery {
        table: table_ref,
        filter,
        returning,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_select() {
        let q = parse_single("SELECT * FROM users").unwrap();
        match q {
            Query::Select(s) => {
                assert_eq!(s.projections.len(), 1);
                assert!(matches!(s.projections[0], SelectItem::Wildcard));
                assert_eq!(s.from.len(), 1);
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_select_with_where() {
        let q = parse_single("SELECT id, name FROM users WHERE age > 18").unwrap();
        match q {
            Query::Select(s) => {
                assert_eq!(s.projections.len(), 2);
                assert!(s.filter.is_some());
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_select_with_join() {
        let q =
            parse_single("SELECT u.name, o.total FROM users u JOIN orders o ON u.id = o.user_id")
                .unwrap();
        match q {
            Query::Select(s) => {
                assert_eq!(s.joins.len(), 1);
                assert!(matches!(s.joins[0].join_type, JoinType::Inner));
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_select_with_group_by() {
        let q = parse_single(
            "SELECT department, COUNT(*) FROM employees GROUP BY department HAVING COUNT(*) > 5",
        )
        .unwrap();
        match q {
            Query::Select(s) => {
                assert_eq!(s.group_by.len(), 1);
                assert!(s.having.is_some());
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_cte() {
        let q = parse_single(
            "WITH active AS (SELECT * FROM users WHERE active = true) SELECT * FROM active",
        )
        .unwrap();
        match q {
            Query::With(cte) => {
                assert!(!cte.recursive);
                assert_eq!(cte.ctes.len(), 1);
                assert_eq!(cte.ctes[0].name, "active");
            }
            _ => panic!("Expected CTE query"),
        }
    }

    #[test]
    fn test_parse_recursive_cte() {
        let q = parse_single(
            "WITH RECURSIVE nums AS (SELECT 1 AS n UNION ALL SELECT n + 1 FROM nums WHERE n < 10) SELECT * FROM nums",
        )
        .unwrap();
        match q {
            Query::With(cte) => {
                assert!(cte.recursive);
            }
            _ => panic!("Expected CTE query"),
        }
    }

    #[test]
    fn test_parse_window_function() {
        let q = parse_single(
            "SELECT name, ROW_NUMBER() OVER (PARTITION BY dept ORDER BY salary DESC) FROM employees",
        )
        .unwrap();
        match q {
            Query::Select(s) => {
                assert_eq!(s.projections.len(), 2);
                match &s.projections[1] {
                    SelectItem::Expression { expr, .. } => {
                        assert!(matches!(expr, Expression::WindowFunction { .. }));
                    }
                    _ => panic!("Expected window function expression"),
                }
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_insert() {
        let q = parse_single("INSERT INTO users (name, email) VALUES ('John', 'john@example.com')")
            .unwrap();
        match q {
            Query::Insert(i) => {
                assert_eq!(i.columns.len(), 2);
                match &i.source {
                    InsertSource::Values(rows) => assert_eq!(rows.len(), 1),
                    _ => panic!("Expected values source"),
                }
            }
            _ => panic!("Expected Insert query"),
        }
    }

    #[test]
    fn test_parse_update() {
        let q = parse_single("UPDATE users SET name = 'Jane' WHERE id = 1").unwrap();
        match q {
            Query::Update(u) => {
                assert_eq!(u.assignments.len(), 1);
                assert_eq!(u.assignments[0].column, "name");
                assert!(u.filter.is_some());
            }
            _ => panic!("Expected Update query"),
        }
    }

    #[test]
    fn test_parse_delete() {
        let q = parse_single("DELETE FROM users WHERE id = 1").unwrap();
        match q {
            Query::Delete(d) => {
                assert!(d.filter.is_some());
            }
            _ => panic!("Expected Delete query"),
        }
    }

    #[test]
    fn test_parse_subquery() {
        let q = parse_single("SELECT * FROM users WHERE id IN (SELECT user_id FROM active_users)")
            .unwrap();
        match q {
            Query::Select(s) => {
                assert!(s.filter.is_some());
                match s.filter.unwrap() {
                    Expression::InSubquery { negated, .. } => {
                        assert!(!negated);
                    }
                    _ => panic!("Expected InSubquery expression"),
                }
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_multiple_statements() {
        let queries = parse_sql("SELECT 1; SELECT 2").unwrap();
        assert_eq!(queries.len(), 2);
    }

    #[test]
    fn test_parse_invalid_sql() {
        assert!(parse_single("SELCT * FORM users").is_err());
    }

    #[test]
    fn test_parse_union() {
        let q = parse_single("SELECT id FROM users UNION ALL SELECT id FROM admins").unwrap();
        match q {
            Query::Select(s) => {
                assert!(s.set_op.is_some());
                let set_op = s.set_op.unwrap();
                assert!(matches!(set_op.op, SetOperator::Union));
                assert!(set_op.all);
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_order_by_limit() {
        let q = parse_single("SELECT * FROM users ORDER BY name ASC LIMIT 10 OFFSET 5").unwrap();
        match q {
            Query::Select(s) => {
                assert_eq!(s.order_by.len(), 1);
                assert_eq!(s.order_by[0].asc, Some(true));
                assert!(s.limit.is_some());
                assert!(s.offset.is_some());
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_between() {
        let q = parse_single("SELECT * FROM products WHERE price BETWEEN 10 AND 100").unwrap();
        match q {
            Query::Select(s) => {
                assert!(matches!(
                    s.filter,
                    Some(Expression::Between { negated: false, .. })
                ));
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_case_expression() {
        let q = parse_single("SELECT CASE WHEN status = 'active' THEN 1 ELSE 0 END FROM users")
            .unwrap();
        match q {
            Query::Select(s) => match &s.projections[0] {
                SelectItem::Expression { expr, .. } => {
                    assert!(matches!(expr, Expression::Case { .. }));
                }
                _ => panic!("Expected expression"),
            },
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_is_null() {
        let q = parse_single("SELECT * FROM users WHERE email IS NOT NULL").unwrap();
        match q {
            Query::Select(s) => {
                assert!(matches!(
                    s.filter,
                    Some(Expression::IsNull { negated: true, .. })
                ));
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_cast() {
        let q = parse_single("SELECT CAST(price AS INTEGER) FROM products").unwrap();
        match q {
            Query::Select(s) => match &s.projections[0] {
                SelectItem::Expression { expr, .. } => {
                    assert!(matches!(expr, Expression::Cast { .. }));
                }
                _ => panic!("Expected expression"),
            },
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_aggregate_distinct() {
        let q = parse_single("SELECT COUNT(DISTINCT status) FROM orders").unwrap();
        match q {
            Query::Select(s) => match &s.projections[0] {
                SelectItem::Expression { expr, .. } => match expr {
                    Expression::Aggregate { distinct, name, .. } => {
                        assert!(distinct);
                        assert_eq!(name, "COUNT");
                    }
                    _ => panic!("Expected aggregate"),
                },
                _ => panic!("Expected expression"),
            },
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_left_join() {
        let q = parse_single("SELECT * FROM a LEFT JOIN b ON a.id = b.a_id").unwrap();
        match q {
            Query::Select(s) => {
                assert_eq!(s.joins.len(), 1);
                assert!(matches!(s.joins[0].join_type, JoinType::Left));
            }
            _ => panic!("Expected Select query"),
        }
    }
}
