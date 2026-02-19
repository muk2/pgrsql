/// SQL pretty-printer / formatter.
///
/// Converts a Query AST into well-indented, readable SQL. Uses the same AST
/// types as the compiler but emits newlines and indentation for each clause.
use super::types::*;

const INDENT: &str = "    ";

/// Format a query AST into pretty-printed PostgreSQL SQL.
pub fn format_sql(query: &Query) -> String {
    match query {
        Query::Select(s) => format_select(s, 0),
        Query::Insert(i) => format_insert(i),
        Query::Update(u) => format_update(u),
        Query::Delete(d) => format_delete(d),
        Query::With(cte) => format_cte(cte),
        Query::Raw(sql) => sql.clone(),
    }
}

fn indent(level: usize) -> String {
    INDENT.repeat(level)
}

fn format_select(select: &SelectQuery, depth: usize) -> String {
    let prefix = indent(depth);
    let mut parts: Vec<String> = Vec::new();

    // SELECT [DISTINCT]
    let mut select_clause = format!("{}SELECT", prefix);
    if select.distinct {
        select_clause.push_str(" DISTINCT");
    }

    if select.projections.is_empty() {
        select_clause.push_str(" *");
    } else if select.projections.len() == 1 {
        select_clause.push_str(&format!(" {}", format_select_item(&select.projections[0])));
    } else {
        for (i, item) in select.projections.iter().enumerate() {
            let comma = if i < select.projections.len() - 1 {
                ","
            } else {
                ""
            };
            select_clause.push_str(&format!(
                "\n{}{}{}",
                prefix,
                INDENT,
                format_select_item(item)
            ));
            select_clause.push_str(comma);
        }
    }
    parts.push(select_clause);

    // FROM
    if !select.from.is_empty() {
        if select.from.len() == 1 {
            parts.push(format!(
                "{}FROM {}",
                prefix,
                format_table_ref(&select.from[0], depth)
            ));
        } else {
            let mut from_clause = format!("{}FROM", prefix);
            for (i, table) in select.from.iter().enumerate() {
                let comma = if i < select.from.len() - 1 { "," } else { "" };
                from_clause.push_str(&format!(
                    "\n{}{}{}{}",
                    prefix,
                    INDENT,
                    format_table_ref(table, depth + 1),
                    comma
                ));
            }
            parts.push(from_clause);
        }
    }

    // JOINs
    for join in &select.joins {
        parts.push(format_join(join, depth));
    }

    // WHERE
    if let Some(ref filter) = select.filter {
        parts.push(format!("{}WHERE {}", prefix, format_expr(filter)));
    }

    // GROUP BY
    if !select.group_by.is_empty() {
        let groups: Vec<String> = select.group_by.iter().map(format_expr).collect();
        parts.push(format!("{}GROUP BY {}", prefix, groups.join(", ")));
    }

    // HAVING
    if let Some(ref having) = select.having {
        parts.push(format!("{}HAVING {}", prefix, format_expr(having)));
    }

    // WINDOW
    for window in &select.windows {
        parts.push(format!(
            "{}WINDOW {} AS ({})",
            prefix,
            window.name,
            format_window_spec(&window.spec)
        ));
    }

    // Set operations
    if let Some(ref set_op) = select.set_op {
        let op_str = match set_op.op {
            SetOperator::Union => "UNION",
            SetOperator::Intersect => "INTERSECT",
            SetOperator::Except => "EXCEPT",
        };
        let all_str = if set_op.all { " ALL" } else { "" };
        parts.push(format!("{}{}{}", prefix, op_str, all_str));
        parts.push(format_sql(&set_op.right));
    }

    // ORDER BY
    if !select.order_by.is_empty() {
        let orders: Vec<String> = select.order_by.iter().map(format_order_by).collect();
        parts.push(format!("{}ORDER BY {}", prefix, orders.join(", ")));
    }

    // LIMIT
    if let Some(ref limit) = select.limit {
        parts.push(format!("{}LIMIT {}", prefix, format_expr(limit)));
    }

    // OFFSET
    if let Some(ref offset) = select.offset {
        parts.push(format!("{}OFFSET {}", prefix, format_expr(offset)));
    }

    parts.join("\n")
}

fn format_select_item(item: &SelectItem) -> String {
    match item {
        SelectItem::Wildcard => "*".to_string(),
        SelectItem::QualifiedWildcard(table) => format!("{}.*", table),
        SelectItem::Expression { expr, alias } => {
            let expr_str = format_expr(expr);
            match alias {
                Some(a) => format!("{} AS {}", expr_str, a),
                None => expr_str,
            }
        }
    }
}

fn format_table_ref(table: &TableRef, depth: usize) -> String {
    match table {
        TableRef::Table {
            schema,
            name,
            alias,
        } => {
            let mut s = match schema {
                Some(sc) => format!("{}.{}", sc, name),
                None => name.clone(),
            };
            if let Some(a) = alias {
                s.push_str(&format!(" AS {}", a));
            }
            s
        }
        TableRef::Subquery { query, alias } => {
            format!("(\n{}\n{}) AS {}", format_sql(query), indent(depth), alias)
        }
        TableRef::Function { name, args, alias } => {
            let args_str: Vec<String> = args.iter().map(format_expr).collect();
            let mut s = format!("{}({})", name, args_str.join(", "));
            if let Some(a) = alias {
                s.push_str(&format!(" AS {}", a));
            }
            s
        }
    }
}

fn format_join(join: &Join, depth: usize) -> String {
    let prefix = indent(depth);
    let type_str = match join.join_type {
        JoinType::Inner => "JOIN",
        JoinType::Left => "LEFT JOIN",
        JoinType::Right => "RIGHT JOIN",
        JoinType::Full => "FULL JOIN",
        JoinType::Cross => "CROSS JOIN",
        JoinType::Lateral => "LATERAL JOIN",
    };

    let table_str = format_table_ref(&join.table, depth + 1);

    let condition_str = match &join.condition {
        Some(JoinCondition::On(expr)) => {
            format!("\n{}{}ON {}", prefix, INDENT, format_expr(expr))
        }
        Some(JoinCondition::Using(cols)) => format!(" USING ({})", cols.join(", ")),
        Some(JoinCondition::Natural) => " NATURAL".to_string(),
        None => String::new(),
    };

    format!("{}{} {}{}", prefix, type_str, table_str, condition_str)
}

fn format_expr(expr: &Expression) -> String {
    match expr {
        Expression::Column { table, name } => match table {
            Some(t) => format!("{}.{}", t, name),
            None => name.clone(),
        },
        Expression::Literal(lit) => format_literal(lit),
        Expression::BinaryOp { left, op, right } => {
            let op_str = match op {
                BinaryOperator::Eq => "=",
                BinaryOperator::NotEq => "<>",
                BinaryOperator::Lt => "<",
                BinaryOperator::LtEq => "<=",
                BinaryOperator::Gt => ">",
                BinaryOperator::GtEq => ">=",
                BinaryOperator::And => "AND",
                BinaryOperator::Or => "OR",
                BinaryOperator::Plus => "+",
                BinaryOperator::Minus => "-",
                BinaryOperator::Multiply => "*",
                BinaryOperator::Divide => "/",
                BinaryOperator::Modulo => "%",
                BinaryOperator::Like => "LIKE",
                BinaryOperator::ILike => "ILIKE",
                BinaryOperator::NotLike => "NOT LIKE",
                BinaryOperator::NotILike => "NOT ILIKE",
                BinaryOperator::Concat => "||",
            };
            format!("{} {} {}", format_expr(left), op_str, format_expr(right))
        }
        Expression::UnaryOp { op, expr } => {
            let op_str = match op {
                UnaryOperator::Not => "NOT",
                UnaryOperator::Minus => "-",
                UnaryOperator::Plus => "+",
            };
            format!("{} {}", op_str, format_expr(expr))
        }
        Expression::Function {
            name,
            args,
            distinct,
        } => {
            let distinct_str = if *distinct { "DISTINCT " } else { "" };
            let args_str: Vec<String> = args.iter().map(format_expr).collect();
            format!("{}({}{})", name, distinct_str, args_str.join(", "))
        }
        Expression::Aggregate {
            name,
            args,
            distinct,
            filter,
        } => {
            let distinct_str = if *distinct { "DISTINCT " } else { "" };
            let args_str: Vec<String> = args.iter().map(format_expr).collect();
            let mut s = format!("{}({}{})", name, distinct_str, args_str.join(", "));
            if let Some(f) = filter {
                s.push_str(&format!(" FILTER (WHERE {})", format_expr(f)));
            }
            s
        }
        Expression::WindowFunction { function, window } => {
            format!(
                "{} OVER ({})",
                format_expr(function),
                format_window_spec(window)
            )
        }
        Expression::Case {
            operand,
            when_clauses,
            else_clause,
        } => {
            let mut s = String::from("CASE");
            if let Some(op) = operand {
                s.push_str(&format!(" {}", format_expr(op)));
            }
            for (when, then) in when_clauses {
                s.push_str(&format!(
                    " WHEN {} THEN {}",
                    format_expr(when),
                    format_expr(then)
                ));
            }
            if let Some(else_expr) = else_clause {
                s.push_str(&format!(" ELSE {}", format_expr(else_expr)));
            }
            s.push_str(" END");
            s
        }
        Expression::Subquery(q) => format!("({})", format_sql(q)),
        Expression::Exists(q) => format!("EXISTS ({})", format_sql(q)),
        Expression::InList {
            expr,
            list,
            negated,
        } => {
            let not_str = if *negated { "NOT " } else { "" };
            let items: Vec<String> = list.iter().map(format_expr).collect();
            format!(
                "{} {}IN ({})",
                format_expr(expr),
                not_str,
                items.join(", ")
            )
        }
        Expression::InSubquery {
            expr,
            subquery,
            negated,
        } => {
            let not_str = if *negated { "NOT " } else { "" };
            format!(
                "{} {}IN ({})",
                format_expr(expr),
                not_str,
                format_sql(subquery)
            )
        }
        Expression::Between {
            expr,
            low,
            high,
            negated,
        } => {
            let not_str = if *negated { "NOT " } else { "" };
            format!(
                "{} {}BETWEEN {} AND {}",
                format_expr(expr),
                not_str,
                format_expr(low),
                format_expr(high)
            )
        }
        Expression::IsNull { expr, negated } => {
            if *negated {
                format!("{} IS NOT NULL", format_expr(expr))
            } else {
                format!("{} IS NULL", format_expr(expr))
            }
        }
        Expression::Cast { expr, data_type } => {
            format!("CAST({} AS {})", format_expr(expr), data_type)
        }
        Expression::Wildcard => "*".to_string(),
        Expression::Parameter(idx) => format!("${}", idx),
        Expression::Array(elems) => {
            let items: Vec<String> = elems.iter().map(format_expr).collect();
            format!("ARRAY[{}]", items.join(", "))
        }
        Expression::JsonAccess {
            expr,
            path,
            as_text,
        } => {
            let op = if *as_text { "->>" } else { "->" };
            format!("{}{}{}", format_expr(expr), op, format_expr(path))
        }
        Expression::TypeCast { expr, data_type } => {
            format!("{}::{}", format_expr(expr), data_type)
        }
        Expression::Nested(expr) => format!("({})", format_expr(expr)),
    }
}

fn format_literal(lit: &Literal) -> String {
    match lit {
        Literal::Null => "NULL".to_string(),
        Literal::Boolean(b) => {
            if *b {
                "TRUE".to_string()
            } else {
                "FALSE".to_string()
            }
        }
        Literal::Integer(i) => i.to_string(),
        Literal::Float(f) => format!("{}", f),
        Literal::String(s) => format!("'{}'", s.replace('\'', "''")),
    }
}

fn format_window_spec(spec: &WindowSpec) -> String {
    let mut parts = Vec::new();

    if !spec.partition_by.is_empty() {
        let cols: Vec<String> = spec.partition_by.iter().map(format_expr).collect();
        parts.push(format!("PARTITION BY {}", cols.join(", ")));
    }

    if !spec.order_by.is_empty() {
        let orders: Vec<String> = spec.order_by.iter().map(format_order_by).collect();
        parts.push(format!("ORDER BY {}", orders.join(", ")));
    }

    if let Some(ref frame) = spec.frame {
        parts.push(format_window_frame(frame));
    }

    parts.join(" ")
}

fn format_window_frame(frame: &WindowFrame) -> String {
    let mode = match frame.mode {
        WindowFrameMode::Rows => "ROWS",
        WindowFrameMode::Range => "RANGE",
        WindowFrameMode::Groups => "GROUPS",
    };

    let start = format_window_frame_bound(&frame.start);

    match &frame.end {
        Some(end) => format!(
            "{} BETWEEN {} AND {}",
            mode,
            start,
            format_window_frame_bound(end)
        ),
        None => format!("{} {}", mode, start),
    }
}

fn format_window_frame_bound(bound: &WindowFrameBound) -> String {
    match bound {
        WindowFrameBound::CurrentRow => "CURRENT ROW".to_string(),
        WindowFrameBound::Preceding(None) => "UNBOUNDED PRECEDING".to_string(),
        WindowFrameBound::Preceding(Some(n)) => format!("{} PRECEDING", n),
        WindowFrameBound::Following(None) => "UNBOUNDED FOLLOWING".to_string(),
        WindowFrameBound::Following(Some(n)) => format!("{} FOLLOWING", n),
    }
}

fn format_order_by(order: &OrderByExpr) -> String {
    let mut s = format_expr(&order.expr);
    match order.asc {
        Some(true) => s.push_str(" ASC"),
        Some(false) => s.push_str(" DESC"),
        None => {}
    }
    match order.nulls_first {
        Some(true) => s.push_str(" NULLS FIRST"),
        Some(false) => s.push_str(" NULLS LAST"),
        None => {}
    }
    s
}

fn format_cte(cte: &CTEQuery) -> String {
    let recursive = if cte.recursive { "RECURSIVE " } else { "" };

    let ctes: Vec<String> = cte
        .ctes
        .iter()
        .map(|c| {
            let cols = if c.columns.is_empty() {
                String::new()
            } else {
                format!("({})", c.columns.join(", "))
            };
            format!(
                "{}{}{} AS (\n{}\n)",
                INDENT,
                c.name,
                cols,
                format_sql(&c.query)
            )
        })
        .collect();

    format!(
        "WITH {}\n{}\n{}",
        recursive,
        ctes.join(",\n"),
        format_sql(&cte.body)
    )
}

fn format_insert(insert: &InsertQuery) -> String {
    let table = format_table_ref(&insert.table, 0);
    let columns = if insert.columns.is_empty() {
        String::new()
    } else {
        format!(" ({})", insert.columns.join(", "))
    };

    let source = match &insert.source {
        InsertSource::Values(rows) => {
            let row_strs: Vec<String> = rows
                .iter()
                .map(|row| {
                    let vals: Vec<String> = row.iter().map(format_expr).collect();
                    format!("{}({})", INDENT, vals.join(", "))
                })
                .collect();
            format!("VALUES\n{}", row_strs.join(",\n"))
        }
        InsertSource::Query(q) => format_sql(q),
    };

    let returning = if insert.returning.is_empty() {
        String::new()
    } else {
        let items: Vec<String> = insert.returning.iter().map(format_select_item).collect();
        format!("\nRETURNING {}", items.join(", "))
    };

    format!("INSERT INTO {}{}\n{}{}", table, columns, source, returning)
}

fn format_update(update: &UpdateQuery) -> String {
    let table = format_table_ref(&update.table, 0);

    let sets: Vec<String> = update
        .assignments
        .iter()
        .map(|a| format!("{}{} = {}", INDENT, a.column, format_expr(&a.value)))
        .collect();

    let filter = match &update.filter {
        Some(f) => format!("\nWHERE {}", format_expr(f)),
        None => String::new(),
    };

    let returning = if update.returning.is_empty() {
        String::new()
    } else {
        let items: Vec<String> = update.returning.iter().map(format_select_item).collect();
        format!("\nRETURNING {}", items.join(", "))
    };

    format!(
        "UPDATE {}\nSET\n{}{}{}",
        table,
        sets.join(",\n"),
        filter,
        returning
    )
}

fn format_delete(delete: &DeleteQuery) -> String {
    let table = format_table_ref(&delete.table, 0);

    let filter = match &delete.filter {
        Some(f) => format!("\nWHERE {}", format_expr(f)),
        None => String::new(),
    };

    let returning = if delete.returning.is_empty() {
        String::new()
    } else {
        let items: Vec<String> = delete.returning.iter().map(format_select_item).collect();
        format!("\nRETURNING {}", items.join(", "))
    };

    format!("DELETE FROM {}{}{}", table, filter, returning)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::parser::parse_single;

    fn format_roundtrip(sql: &str) -> String {
        let query = parse_single(sql).expect("Failed to parse");
        format_sql(&query)
    }

    #[test]
    fn test_format_simple_select() {
        let result = format_roundtrip("SELECT * FROM users");
        assert!(result.contains("SELECT *"));
        assert!(result.contains("FROM users"));
        // Verify it's multi-line
        assert!(result.contains('\n'));
    }

    #[test]
    fn test_format_multicolumn_select() {
        let result = format_roundtrip("SELECT id, name, email FROM users");
        // Each column should be on its own line
        assert!(result.contains("id,"));
        assert!(result.contains("name,"));
        assert!(result.contains("email"));
    }

    #[test]
    fn test_format_select_with_where() {
        let result = format_roundtrip("SELECT id, name FROM users WHERE age > 18");
        assert!(result.contains("WHERE"));
        assert!(result.contains("age > 18"));
    }

    #[test]
    fn test_format_join() {
        let result =
            format_roundtrip("SELECT * FROM users JOIN orders ON users.id = orders.user_id");
        assert!(result.contains("JOIN orders"));
        assert!(result.contains("ON users.id = orders.user_id"));
    }

    #[test]
    fn test_format_group_by_having() {
        let result = format_roundtrip(
            "SELECT dept, COUNT(*) FROM emp GROUP BY dept HAVING COUNT(*) > 5",
        );
        assert!(result.contains("GROUP BY"));
        assert!(result.contains("HAVING"));
    }

    #[test]
    fn test_format_order_by_limit() {
        let result = format_roundtrip("SELECT * FROM users ORDER BY name ASC LIMIT 10 OFFSET 5");
        assert!(result.contains("ORDER BY"));
        assert!(result.contains("LIMIT 10"));
        assert!(result.contains("OFFSET 5"));
    }

    #[test]
    fn test_format_cte() {
        let result = format_roundtrip(
            "WITH active AS (SELECT * FROM users WHERE active = TRUE) SELECT * FROM active",
        );
        assert!(result.contains("WITH"));
        assert!(result.contains("active AS"));
    }

    #[test]
    fn test_format_insert() {
        let result =
            format_roundtrip("INSERT INTO users (name, email) VALUES ('John', 'john@example.com')");
        assert!(result.contains("INSERT INTO"));
        assert!(result.contains("VALUES"));
    }

    #[test]
    fn test_format_update() {
        let result = format_roundtrip("UPDATE users SET name = 'Jane' WHERE id = 1");
        assert!(result.contains("UPDATE"));
        assert!(result.contains("SET"));
        assert!(result.contains("WHERE"));
    }

    #[test]
    fn test_format_delete() {
        let result = format_roundtrip("DELETE FROM users WHERE id = 1");
        assert!(result.contains("DELETE FROM"));
        assert!(result.contains("WHERE"));
    }

    #[test]
    fn test_format_is_reparseable() {
        let test_cases = vec![
            "SELECT * FROM users",
            "SELECT id, name FROM users WHERE age > 18",
            "SELECT * FROM users ORDER BY name LIMIT 10",
            "INSERT INTO users (name) VALUES ('John')",
            "UPDATE users SET name = 'Jane' WHERE id = 1",
            "DELETE FROM users WHERE id = 1",
        ];

        for sql in test_cases {
            let formatted = format_roundtrip(sql);
            let reparsed = parse_single(&formatted);
            assert!(
                reparsed.is_ok(),
                "Formatted SQL not reparseable: {} -> {} -> {:?}",
                sql,
                formatted,
                reparsed.err()
            );
        }
    }

    #[test]
    fn test_format_produces_newlines() {
        let result = format_roundtrip("SELECT id, name FROM users WHERE age > 18 ORDER BY name");
        let lines: Vec<&str> = result.lines().collect();
        // Should have at least SELECT, FROM, WHERE, ORDER BY on separate lines
        assert!(
            lines.len() >= 4,
            "Expected at least 4 lines, got {}:\n{}",
            lines.len(),
            result
        );
    }

    #[test]
    fn test_format_window_function() {
        let result = format_roundtrip(
            "SELECT name, ROW_NUMBER() OVER (PARTITION BY dept ORDER BY salary DESC) FROM employees",
        );
        assert!(result.contains("OVER"));
        assert!(result.contains("PARTITION BY"));
    }

    #[test]
    fn test_format_union() {
        let result = format_roundtrip("SELECT id FROM users UNION ALL SELECT id FROM admins");
        assert!(result.contains("UNION ALL"));
    }

    #[test]
    fn test_format_left_join() {
        let result = format_roundtrip("SELECT * FROM a LEFT JOIN b ON a.id = b.a_id");
        assert!(result.contains("LEFT JOIN"));
    }

    #[test]
    fn test_format_subquery() {
        let result = format_roundtrip(
            "SELECT * FROM users WHERE id IN (SELECT user_id FROM active_users)",
        );
        assert!(result.contains("IN ("));
    }
}
