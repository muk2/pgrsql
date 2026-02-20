/// Unified AST → SQL compiler.
///
/// Converts our internal AST back into a SQL string targeting PostgreSQL.
/// This enables round-trip parsing: SQL → AST → SQL, and allows DSLs
/// to generate SQL by building AST nodes.
use super::types::*;

/// Compile a query AST into a PostgreSQL SQL string.
pub fn compile(query: &Query) -> String {
    match query {
        Query::Select(s) => compile_select(s),
        Query::Insert(i) => compile_insert(i),
        Query::Update(u) => compile_update(u),
        Query::Delete(d) => compile_delete(d),
        Query::With(cte) => compile_cte(cte),
        Query::Raw(sql) => sql.clone(),
    }
}

fn compile_select(select: &SelectQuery) -> String {
    let mut parts = Vec::new();

    // SELECT [DISTINCT]
    let mut select_clause = String::from("SELECT ");
    if select.distinct {
        select_clause.push_str("DISTINCT ");
    }

    if select.projections.is_empty() {
        select_clause.push('*');
    } else {
        let items: Vec<String> = select.projections.iter().map(compile_select_item).collect();
        select_clause.push_str(&items.join(", "));
    }
    parts.push(select_clause);

    // FROM
    if !select.from.is_empty() {
        let tables: Vec<String> = select.from.iter().map(compile_table_ref).collect();
        parts.push(format!("FROM {}", tables.join(", ")));
    }

    // JOINs
    for join in &select.joins {
        parts.push(compile_join(join));
    }

    // WHERE
    if let Some(ref filter) = select.filter {
        parts.push(format!("WHERE {}", compile_expr(filter)));
    }

    // GROUP BY
    if !select.group_by.is_empty() {
        let groups: Vec<String> = select.group_by.iter().map(compile_expr).collect();
        parts.push(format!("GROUP BY {}", groups.join(", ")));
    }

    // HAVING
    if let Some(ref having) = select.having {
        parts.push(format!("HAVING {}", compile_expr(having)));
    }

    // WINDOW
    for window in &select.windows {
        parts.push(format!(
            "WINDOW {} AS ({})",
            window.name,
            compile_window_spec(&window.spec)
        ));
    }

    // Set operations (UNION, INTERSECT, EXCEPT)
    if let Some(ref set_op) = select.set_op {
        let op_str = match set_op.op {
            SetOperator::Union => "UNION",
            SetOperator::Intersect => "INTERSECT",
            SetOperator::Except => "EXCEPT",
        };
        let all_str = if set_op.all { " ALL" } else { "" };
        parts.push(format!("{}{} {}", op_str, all_str, compile(&set_op.right)));
    }

    // ORDER BY
    if !select.order_by.is_empty() {
        let orders: Vec<String> = select.order_by.iter().map(compile_order_by).collect();
        parts.push(format!("ORDER BY {}", orders.join(", ")));
    }

    // LIMIT
    if let Some(ref limit) = select.limit {
        parts.push(format!("LIMIT {}", compile_expr(limit)));
    }

    // OFFSET
    if let Some(ref offset) = select.offset {
        parts.push(format!("OFFSET {}", compile_expr(offset)));
    }

    parts.join(" ")
}

fn compile_select_item(item: &SelectItem) -> String {
    match item {
        SelectItem::Wildcard => "*".to_string(),
        SelectItem::QualifiedWildcard(table) => format!("{}.*", table),
        SelectItem::Expression { expr, alias } => {
            let expr_str = compile_expr(expr);
            match alias {
                Some(a) => format!("{} AS {}", expr_str, a),
                None => expr_str,
            }
        }
    }
}

fn compile_table_ref(table: &TableRef) -> String {
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
            format!("({}) AS {}", compile(query), alias)
        }
        TableRef::Function { name, args, alias } => {
            let args_str: Vec<String> = args.iter().map(compile_expr).collect();
            let mut s = format!("{}({})", name, args_str.join(", "));
            if let Some(a) = alias {
                s.push_str(&format!(" AS {}", a));
            }
            s
        }
    }
}

fn compile_join(join: &Join) -> String {
    let type_str = match join.join_type {
        JoinType::Inner => "JOIN",
        JoinType::Left => "LEFT JOIN",
        JoinType::Right => "RIGHT JOIN",
        JoinType::Full => "FULL JOIN",
        JoinType::Cross => "CROSS JOIN",
        JoinType::Lateral => "LATERAL JOIN",
    };

    let table_str = compile_table_ref(&join.table);

    let condition_str = match &join.condition {
        Some(JoinCondition::On(expr)) => format!(" ON {}", compile_expr(expr)),
        Some(JoinCondition::Using(cols)) => format!(" USING ({})", cols.join(", ")),
        Some(JoinCondition::Natural) => " NATURAL".to_string(),
        None => String::new(),
    };

    format!("{} {}{}", type_str, table_str, condition_str)
}

fn compile_expr(expr: &Expression) -> String {
    match expr {
        Expression::Column { table, name } => match table {
            Some(t) => format!("{}.{}", t, name),
            None => name.clone(),
        },
        Expression::Literal(lit) => compile_literal(lit),
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
            format!("{} {} {}", compile_expr(left), op_str, compile_expr(right))
        }
        Expression::UnaryOp { op, expr } => {
            let op_str = match op {
                UnaryOperator::Not => "NOT",
                UnaryOperator::Minus => "-",
                UnaryOperator::Plus => "+",
            };
            format!("{} {}", op_str, compile_expr(expr))
        }
        Expression::Function {
            name,
            args,
            distinct,
        } => {
            let distinct_str = if *distinct { "DISTINCT " } else { "" };
            let args_str: Vec<String> = args.iter().map(compile_expr).collect();
            format!("{}({}{})", name, distinct_str, args_str.join(", "))
        }
        Expression::Aggregate {
            name,
            args,
            distinct,
            filter,
        } => {
            let distinct_str = if *distinct { "DISTINCT " } else { "" };
            let args_str: Vec<String> = args.iter().map(compile_expr).collect();
            let mut s = format!("{}({}{})", name, distinct_str, args_str.join(", "));
            if let Some(f) = filter {
                s.push_str(&format!(" FILTER (WHERE {})", compile_expr(f)));
            }
            s
        }
        Expression::WindowFunction { function, window } => {
            format!(
                "{} OVER ({})",
                compile_expr(function),
                compile_window_spec(window)
            )
        }
        Expression::Case {
            operand,
            when_clauses,
            else_clause,
        } => {
            let mut s = String::from("CASE");
            if let Some(op) = operand {
                s.push_str(&format!(" {}", compile_expr(op)));
            }
            for (when, then) in when_clauses {
                s.push_str(&format!(
                    " WHEN {} THEN {}",
                    compile_expr(when),
                    compile_expr(then)
                ));
            }
            if let Some(else_expr) = else_clause {
                s.push_str(&format!(" ELSE {}", compile_expr(else_expr)));
            }
            s.push_str(" END");
            s
        }
        Expression::Subquery(q) => format!("({})", compile(q)),
        Expression::Exists(q) => format!("EXISTS ({})", compile(q)),
        Expression::InList {
            expr,
            list,
            negated,
        } => {
            let not_str = if *negated { "NOT " } else { "" };
            let items: Vec<String> = list.iter().map(compile_expr).collect();
            format!(
                "{} {}IN ({})",
                compile_expr(expr),
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
                compile_expr(expr),
                not_str,
                compile(subquery)
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
                compile_expr(expr),
                not_str,
                compile_expr(low),
                compile_expr(high)
            )
        }
        Expression::IsNull { expr, negated } => {
            if *negated {
                format!("{} IS NOT NULL", compile_expr(expr))
            } else {
                format!("{} IS NULL", compile_expr(expr))
            }
        }
        Expression::Cast { expr, data_type } => {
            format!("CAST({} AS {})", compile_expr(expr), data_type)
        }
        Expression::Wildcard => "*".to_string(),
        Expression::Parameter(idx) => format!("${}", idx),
        Expression::Array(elems) => {
            let items: Vec<String> = elems.iter().map(compile_expr).collect();
            format!("ARRAY[{}]", items.join(", "))
        }
        Expression::JsonAccess {
            expr,
            path,
            as_text,
        } => {
            let op = if *as_text { "->>" } else { "->" };
            format!("{}{}{}", compile_expr(expr), op, compile_expr(path))
        }
        Expression::TypeCast { expr, data_type } => {
            format!("{}::{}", compile_expr(expr), data_type)
        }
        Expression::Nested(expr) => format!("({})", compile_expr(expr)),
    }
}

fn compile_literal(lit: &Literal) -> String {
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

fn compile_window_spec(spec: &WindowSpec) -> String {
    let mut parts = Vec::new();

    if !spec.partition_by.is_empty() {
        let cols: Vec<String> = spec.partition_by.iter().map(compile_expr).collect();
        parts.push(format!("PARTITION BY {}", cols.join(", ")));
    }

    if !spec.order_by.is_empty() {
        let orders: Vec<String> = spec.order_by.iter().map(compile_order_by).collect();
        parts.push(format!("ORDER BY {}", orders.join(", ")));
    }

    if let Some(ref frame) = spec.frame {
        parts.push(compile_window_frame(frame));
    }

    parts.join(" ")
}

fn compile_window_frame(frame: &WindowFrame) -> String {
    let mode = match frame.mode {
        WindowFrameMode::Rows => "ROWS",
        WindowFrameMode::Range => "RANGE",
        WindowFrameMode::Groups => "GROUPS",
    };

    let start = compile_window_frame_bound(&frame.start);

    match &frame.end {
        Some(end) => format!(
            "{} BETWEEN {} AND {}",
            mode,
            start,
            compile_window_frame_bound(end)
        ),
        None => format!("{} {}", mode, start),
    }
}

fn compile_window_frame_bound(bound: &WindowFrameBound) -> String {
    match bound {
        WindowFrameBound::CurrentRow => "CURRENT ROW".to_string(),
        WindowFrameBound::Preceding(None) => "UNBOUNDED PRECEDING".to_string(),
        WindowFrameBound::Preceding(Some(n)) => format!("{} PRECEDING", n),
        WindowFrameBound::Following(None) => "UNBOUNDED FOLLOWING".to_string(),
        WindowFrameBound::Following(Some(n)) => format!("{} FOLLOWING", n),
    }
}

fn compile_order_by(order: &OrderByExpr) -> String {
    let mut s = compile_expr(&order.expr);
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

fn compile_cte(cte: &CTEQuery) -> String {
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
            format!("{}{} AS ({})", c.name, cols, compile(&c.query))
        })
        .collect();

    format!(
        "WITH {}{} {}",
        recursive,
        ctes.join(", "),
        compile(&cte.body)
    )
}

fn compile_insert(insert: &InsertQuery) -> String {
    let table = compile_table_ref(&insert.table);
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
                    let vals: Vec<String> = row.iter().map(compile_expr).collect();
                    format!("({})", vals.join(", "))
                })
                .collect();
            format!("VALUES {}", row_strs.join(", "))
        }
        InsertSource::Query(q) => compile(q),
    };

    let returning = if insert.returning.is_empty() {
        String::new()
    } else {
        let items: Vec<String> = insert.returning.iter().map(compile_select_item).collect();
        format!(" RETURNING {}", items.join(", "))
    };

    format!("INSERT INTO {}{} {}{}", table, columns, source, returning)
}

fn compile_update(update: &UpdateQuery) -> String {
    let table = compile_table_ref(&update.table);

    let sets: Vec<String> = update
        .assignments
        .iter()
        .map(|a| format!("{} = {}", a.column, compile_expr(&a.value)))
        .collect();

    let filter = match &update.filter {
        Some(f) => format!(" WHERE {}", compile_expr(f)),
        None => String::new(),
    };

    let returning = if update.returning.is_empty() {
        String::new()
    } else {
        let items: Vec<String> = update.returning.iter().map(compile_select_item).collect();
        format!(" RETURNING {}", items.join(", "))
    };

    format!(
        "UPDATE {} SET {}{}{}",
        table,
        sets.join(", "),
        filter,
        returning
    )
}

fn compile_delete(delete: &DeleteQuery) -> String {
    let table = compile_table_ref(&delete.table);

    let filter = match &delete.filter {
        Some(f) => format!(" WHERE {}", compile_expr(f)),
        None => String::new(),
    };

    let returning = if delete.returning.is_empty() {
        String::new()
    } else {
        let items: Vec<String> = delete.returning.iter().map(compile_select_item).collect();
        format!(" RETURNING {}", items.join(", "))
    };

    format!("DELETE FROM {}{}{}", table, filter, returning)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::parser::parse_single;

    /// Helper: parse SQL, compile back, and verify the result parses again.
    fn round_trip(sql: &str) -> String {
        let query = parse_single(sql).expect("Failed to parse");
        compile(&query)
    }

    #[test]
    fn test_compile_simple_select() {
        let compiled = round_trip("SELECT * FROM users");
        assert!(compiled.contains("SELECT"));
        assert!(compiled.contains("FROM users"));
    }

    #[test]
    fn test_compile_select_with_where() {
        let compiled = round_trip("SELECT id, name FROM users WHERE age > 18");
        assert!(compiled.contains("WHERE"));
        assert!(compiled.contains("age > 18"));
    }

    #[test]
    fn test_compile_select_with_alias() {
        let compiled = round_trip("SELECT u.name AS user_name FROM users AS u");
        assert!(compiled.contains("AS user_name"));
        assert!(compiled.contains("AS u"));
    }

    #[test]
    fn test_compile_join() {
        let compiled = round_trip("SELECT * FROM users JOIN orders ON users.id = orders.user_id");
        assert!(compiled.contains("JOIN"));
        assert!(compiled.contains("ON"));
    }

    #[test]
    fn test_compile_left_join() {
        let compiled = round_trip("SELECT * FROM a LEFT JOIN b ON a.id = b.a_id");
        assert!(compiled.contains("LEFT JOIN"));
    }

    #[test]
    fn test_compile_group_by_having() {
        let compiled =
            round_trip("SELECT dept, COUNT(*) FROM emp GROUP BY dept HAVING COUNT(*) > 5");
        assert!(compiled.contains("GROUP BY"));
        assert!(compiled.contains("HAVING"));
    }

    #[test]
    fn test_compile_order_by_limit() {
        let compiled = round_trip("SELECT * FROM users ORDER BY name ASC LIMIT 10 OFFSET 5");
        assert!(compiled.contains("ORDER BY"));
        assert!(compiled.contains("LIMIT 10"));
        assert!(compiled.contains("OFFSET 5"));
    }

    #[test]
    fn test_compile_cte() {
        let compiled = round_trip(
            "WITH active AS (SELECT * FROM users WHERE active = TRUE) SELECT * FROM active",
        );
        assert!(compiled.contains("WITH "));
        assert!(compiled.contains("active AS"));
    }

    #[test]
    fn test_compile_insert() {
        let compiled =
            round_trip("INSERT INTO users (name, email) VALUES ('John', 'john@example.com')");
        assert!(compiled.contains("INSERT INTO"));
        assert!(compiled.contains("VALUES"));
    }

    #[test]
    fn test_compile_update() {
        let compiled = round_trip("UPDATE users SET name = 'Jane' WHERE id = 1");
        assert!(compiled.contains("UPDATE"));
        assert!(compiled.contains("SET"));
        assert!(compiled.contains("WHERE"));
    }

    #[test]
    fn test_compile_delete() {
        let compiled = round_trip("DELETE FROM users WHERE id = 1");
        assert!(compiled.contains("DELETE FROM"));
        assert!(compiled.contains("WHERE"));
    }

    #[test]
    fn test_compile_union() {
        let compiled = round_trip("SELECT id FROM users UNION ALL SELECT id FROM admins");
        assert!(compiled.contains("UNION ALL"));
    }

    #[test]
    fn test_compile_between() {
        let compiled = round_trip("SELECT * FROM products WHERE price BETWEEN 10 AND 100");
        assert!(compiled.contains("BETWEEN"));
    }

    #[test]
    fn test_compile_is_null() {
        let compiled = round_trip("SELECT * FROM users WHERE email IS NOT NULL");
        assert!(compiled.contains("IS NOT NULL"));
    }

    #[test]
    fn test_compile_case() {
        let compiled =
            round_trip("SELECT CASE WHEN status = 'active' THEN 1 ELSE 0 END FROM users");
        assert!(compiled.contains("CASE"));
        assert!(compiled.contains("WHEN"));
        assert!(compiled.contains("THEN"));
        assert!(compiled.contains("ELSE"));
        assert!(compiled.contains("END"));
    }

    #[test]
    fn test_compile_window_function() {
        let compiled = round_trip(
            "SELECT ROW_NUMBER() OVER (PARTITION BY dept ORDER BY salary DESC) FROM employees",
        );
        assert!(compiled.contains("OVER"));
        assert!(compiled.contains("PARTITION BY"));
    }

    #[test]
    fn test_compile_subquery_in() {
        let compiled =
            round_trip("SELECT * FROM users WHERE id IN (SELECT user_id FROM active_users)");
        assert!(compiled.contains("IN ("));
    }

    #[test]
    fn test_compile_distinct() {
        let compiled = round_trip("SELECT DISTINCT name FROM users");
        assert!(compiled.contains("DISTINCT"));
    }

    #[test]
    fn test_compile_aggregate_distinct() {
        let compiled = round_trip("SELECT COUNT(DISTINCT status) FROM orders");
        assert!(compiled.contains("COUNT(DISTINCT"));
    }

    #[test]
    fn test_round_trip_reparseable() {
        let test_cases = vec![
            "SELECT * FROM users",
            "SELECT id, name FROM users WHERE age > 18",
            "SELECT * FROM users ORDER BY name LIMIT 10",
            "INSERT INTO users (name) VALUES ('John')",
            "UPDATE users SET name = 'Jane' WHERE id = 1",
            "DELETE FROM users WHERE id = 1",
        ];

        for sql in test_cases {
            let compiled = round_trip(sql);
            // The compiled SQL should be parseable again
            let reparsed = parse_single(&compiled);
            assert!(
                reparsed.is_ok(),
                "Round-trip failed for: {} -> {} -> {:?}",
                sql,
                compiled,
                reparsed.err()
            );
        }
    }
}
