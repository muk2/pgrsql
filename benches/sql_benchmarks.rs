//! Comprehensive benchmark suite for pgrsql's SQL processing pipeline.
//!
//! Benchmarks cover:
//! - SQL parsing (text → AST)
//! - AST optimization passes
//! - SQL compilation (AST → text)
//! - Full round-trip (parse → optimize → compile)
//! - EXPLAIN plan parsing
//!
//! Run with: `cargo bench`

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use pgrsql::ast::{compile, parse_single, parse_sql, Optimizer};
use pgrsql::explain::parse_explain_output;

// ---------------------------------------------------------------------------
// SQL test inputs organized by complexity
// ---------------------------------------------------------------------------

const SIMPLE_SELECT: &str = "SELECT * FROM users";

const SELECT_WITH_WHERE: &str =
    "SELECT id, name, email FROM users WHERE age > 18 AND status = 'active'";

const SELECT_WITH_JOIN: &str = "SELECT u.name, o.total, o.created_at \
    FROM users u \
    JOIN orders o ON u.id = o.user_id \
    WHERE o.total > 100.00 \
    ORDER BY o.created_at DESC \
    LIMIT 50";

const MULTI_JOIN: &str = "SELECT u.name, o.id, p.name AS product, oi.quantity \
    FROM users u \
    JOIN orders o ON u.id = o.user_id \
    JOIN order_items oi ON o.id = oi.order_id \
    JOIN products p ON oi.product_id = p.id \
    WHERE o.status = 'completed' AND u.active = true \
    ORDER BY o.created_at DESC";

const AGGREGATION: &str = "SELECT department, COUNT(*) AS emp_count, \
    AVG(salary) AS avg_salary, MAX(salary) AS max_salary, MIN(salary) AS min_salary \
    FROM employees \
    WHERE hire_date > '2020-01-01' \
    GROUP BY department \
    HAVING COUNT(*) > 5 \
    ORDER BY avg_salary DESC";

const CTE_QUERY: &str = "WITH active_users AS (\
        SELECT id, name, email FROM users WHERE status = 'active'\
    ), user_orders AS (\
        SELECT u.id, u.name, COUNT(o.id) AS order_count, SUM(o.total) AS total_spent \
        FROM active_users u \
        JOIN orders o ON u.id = o.user_id \
        GROUP BY u.id, u.name\
    ) \
    SELECT name, order_count, total_spent \
    FROM user_orders \
    WHERE total_spent > 1000 \
    ORDER BY total_spent DESC";

const WINDOW_FUNCTION: &str = "SELECT name, department, salary, \
    ROW_NUMBER() OVER (PARTITION BY department ORDER BY salary DESC) AS rank, \
    AVG(salary) OVER (PARTITION BY department) AS dept_avg, \
    SUM(salary) OVER (ORDER BY hire_date ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) AS running_total \
    FROM employees";

const SUBQUERY: &str = "SELECT u.name, u.email \
    FROM users u \
    WHERE u.id IN (SELECT DISTINCT user_id FROM orders WHERE total > 500) \
    AND u.department = (SELECT department FROM departments WHERE name = 'Engineering') \
    AND EXISTS (SELECT 1 FROM reviews r WHERE r.user_id = u.id AND r.rating > 4)";

const UNION_QUERY: &str = "SELECT id, name, 'customer' AS type FROM customers WHERE active = true \
    UNION ALL \
    SELECT id, name, 'supplier' AS type FROM suppliers WHERE active = true \
    UNION ALL \
    SELECT id, name, 'partner' AS type FROM partners WHERE active = true";

const INSERT_QUERY: &str =
    "INSERT INTO users (name, email, age, department) VALUES ('John Doe', 'john@example.com', 30, 'Engineering')";

const UPDATE_QUERY: &str = "UPDATE employees SET salary = salary * 1.10, \
    updated_at = CURRENT_TIMESTAMP \
    WHERE department = 'Engineering' AND performance_rating > 4";

const DELETE_QUERY: &str =
    "DELETE FROM sessions WHERE last_active < CURRENT_TIMESTAMP - INTERVAL '30 days'";

const CASE_EXPRESSION: &str = "SELECT name, \
    CASE \
        WHEN salary > 100000 THEN 'senior' \
        WHEN salary > 60000 THEN 'mid' \
        WHEN salary > 30000 THEN 'junior' \
        ELSE 'intern' \
    END AS level, \
    CASE department \
        WHEN 'Engineering' THEN 'tech' \
        WHEN 'Marketing' THEN 'business' \
        ELSE 'other' \
    END AS category \
    FROM employees";

// ---------------------------------------------------------------------------
// EXPLAIN plan test input
// ---------------------------------------------------------------------------

const EXPLAIN_OUTPUT: &str = "\
Sort  (cost=100.00..100.25 rows=100 width=40) (actual time=0.500..0.800 rows=95 loops=1)
  Sort Key: name
  Sort Method: quicksort  Memory: 32kB
  ->  Hash Join  (cost=30.00..96.50 rows=100 width=40) (actual time=0.200..0.400 rows=95 loops=1)
        Hash Cond: (o.user_id = u.id)
        ->  Seq Scan on orders o  (cost=0.00..55.00 rows=500 width=20) (actual time=0.010..0.100 rows=500 loops=1)
              Filter: (total > 100.00)
              Rows Removed by Filter: 250
        ->  Hash  (cost=25.00..25.00 rows=400 width=24) (actual time=0.150..0.150 rows=400 loops=1)
              Buckets: 1024  Batches: 1  Memory Usage: 32kB
              ->  Seq Scan on users u  (cost=0.00..25.00 rows=400 width=24) (actual time=0.005..0.080 rows=400 loops=1)
                    Filter: (active = true)
                    Rows Removed by Filter: 100
Planning Time: 0.250 ms
Execution Time: 1.200 ms";

// ---------------------------------------------------------------------------
// Benchmark groups
// ---------------------------------------------------------------------------

fn bench_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("parsing");

    let cases = [
        ("simple_select", SIMPLE_SELECT),
        ("select_where", SELECT_WITH_WHERE),
        ("select_join", SELECT_WITH_JOIN),
        ("multi_join", MULTI_JOIN),
        ("aggregation", AGGREGATION),
        ("cte", CTE_QUERY),
        ("window_function", WINDOW_FUNCTION),
        ("subquery", SUBQUERY),
        ("union", UNION_QUERY),
        ("insert", INSERT_QUERY),
        ("update", UPDATE_QUERY),
        ("delete", DELETE_QUERY),
        ("case_expression", CASE_EXPRESSION),
    ];

    for (name, sql) in &cases {
        group.bench_with_input(BenchmarkId::new("parse", name), sql, |b, sql| {
            b.iter(|| parse_single(black_box(sql)).unwrap());
        });
    }

    group.finish();
}

fn bench_compilation(c: &mut Criterion) {
    let mut group = c.benchmark_group("compilation");

    let cases = [
        ("simple_select", SIMPLE_SELECT),
        ("select_where", SELECT_WITH_WHERE),
        ("select_join", SELECT_WITH_JOIN),
        ("multi_join", MULTI_JOIN),
        ("aggregation", AGGREGATION),
        ("cte", CTE_QUERY),
        ("window_function", WINDOW_FUNCTION),
        ("subquery", SUBQUERY),
        ("union", UNION_QUERY),
        ("insert", INSERT_QUERY),
        ("update", UPDATE_QUERY),
        ("delete", DELETE_QUERY),
        ("case_expression", CASE_EXPRESSION),
    ];

    for (name, sql) in &cases {
        let ast = parse_single(sql).unwrap();
        group.bench_with_input(BenchmarkId::new("compile", name), &ast, |b, ast| {
            b.iter(|| compile(black_box(ast)));
        });
    }

    group.finish();
}

fn bench_optimization(c: &mut Criterion) {
    let mut group = c.benchmark_group("optimization");

    let optimizer = Optimizer::with_defaults();

    let cases = [
        ("simple_select", SIMPLE_SELECT),
        ("select_where", SELECT_WITH_WHERE),
        ("multi_join", MULTI_JOIN),
        ("aggregation", AGGREGATION),
        ("cte", CTE_QUERY),
        ("window_function", WINDOW_FUNCTION),
        ("subquery", SUBQUERY),
    ];

    for (name, sql) in &cases {
        let ast = parse_single(sql).unwrap();
        group.bench_with_input(
            BenchmarkId::new("optimize", name),
            &ast,
            |b, ast| {
                b.iter(|| optimizer.optimize(black_box(ast.clone())).unwrap());
            },
        );
    }

    group.finish();
}

fn bench_round_trip(c: &mut Criterion) {
    let mut group = c.benchmark_group("round_trip");

    let optimizer = Optimizer::with_defaults();

    let cases = [
        ("simple_select", SIMPLE_SELECT),
        ("select_join", SELECT_WITH_JOIN),
        ("multi_join", MULTI_JOIN),
        ("cte", CTE_QUERY),
        ("window_function", WINDOW_FUNCTION),
        ("subquery", SUBQUERY),
    ];

    for (name, sql) in &cases {
        group.bench_with_input(
            BenchmarkId::new("parse_optimize_compile", name),
            sql,
            |b, sql| {
                b.iter(|| {
                    let ast = parse_single(black_box(sql)).unwrap();
                    let optimized = optimizer.optimize(ast).unwrap();
                    compile(&optimized)
                });
            },
        );
    }

    group.finish();
}

fn bench_multi_statement(c: &mut Criterion) {
    let mut group = c.benchmark_group("multi_statement");

    let two_stmts = format!("{}; {}", SIMPLE_SELECT, SELECT_WITH_WHERE);
    let five_stmts = format!(
        "{}; {}; {}; {}; {}",
        SIMPLE_SELECT, SELECT_WITH_WHERE, INSERT_QUERY, UPDATE_QUERY, DELETE_QUERY
    );

    group.bench_function("parse_2_statements", |b| {
        b.iter(|| parse_sql(black_box(&two_stmts)).unwrap());
    });

    group.bench_function("parse_5_statements", |b| {
        b.iter(|| parse_sql(black_box(&five_stmts)).unwrap());
    });

    group.finish();
}

fn bench_explain_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("explain");

    group.bench_function("parse_explain_plan", |b| {
        b.iter(|| parse_explain_output(black_box(EXPLAIN_OUTPUT)));
    });

    // Larger explain plan
    let large_plan = format!(
        "{}\n{}\n{}",
        EXPLAIN_OUTPUT, EXPLAIN_OUTPUT, EXPLAIN_OUTPUT
    );
    group.bench_function("parse_explain_plan_large", |b| {
        b.iter(|| parse_explain_output(black_box(&large_plan)));
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_parsing,
    bench_compilation,
    bench_optimization,
    bench_round_trip,
    bench_multi_statement,
    bench_explain_parsing,
);
criterion_main!(benches);
