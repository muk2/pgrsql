"""pgrsql - High-performance SQL parsing, optimization, and analysis engine.

Built on a Rust core with PyO3 bindings for maximum performance.

Usage:
    import pgrsql

    # Parse SQL to normalized form
    statements = pgrsql.parse_sql("SELECT * FROM users WHERE age > 18")

    # Format and optimize SQL
    formatted = pgrsql.format_sql("select id,name from users where age>18")

    # Analyze query structure
    analysis = pgrsql.analyze_query("SELECT * FROM a JOIN b ON a.id = b.id")
    # {'has_select': True, 'has_joins': True, 'join_count': 1, ...}

    # Parse EXPLAIN output
    plan = pgrsql.parse_explain("Seq Scan on users (cost=0.00..35.50 rows=100 width=36)")

    # Check if a query is an EXPLAIN query
    pgrsql.is_explain_query("EXPLAIN SELECT 1")  # True
"""

from pgrsql._pgrsql import (
    __version__,
    analyze_query,
    format_sql,
    is_explain_query,
    parse_explain,
    parse_sql,
)

__all__ = [
    "__version__",
    "parse_sql",
    "format_sql",
    "analyze_query",
    "parse_explain",
    "is_explain_query",
]
