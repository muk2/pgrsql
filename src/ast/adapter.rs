/// Language adapter traits for the multi-language execution engine.
///
/// Each language adapter converts its input format into our unified AST.
/// This enables pgrsql to accept queries from SQL, Python DSLs, Rust DSLs,
/// and future language integrations.
use anyhow::Result;

use super::types::Query;

/// Adapter for parsing raw query languages (like SQL dialects).
///
/// Implementations should handle a specific language's text format
/// and produce our unified AST.
///
/// # Example
///
/// ```ignore
/// struct PostgresAdapter;
///
/// impl QueryLanguageAdapter for PostgresAdapter {
///     fn name(&self) -> &str { "PostgreSQL" }
///     fn parse(&self, input: &str) -> Result<Vec<Query>> {
///         crate::ast::parser::parse_sql(input)
///     }
/// }
/// ```
pub trait QueryLanguageAdapter: Send + Sync {
    /// Human-readable name of the language this adapter handles.
    fn name(&self) -> &str;

    /// Parse input text into one or more AST queries.
    fn parse(&self, input: &str) -> Result<Vec<Query>>;

    /// Check if this adapter can handle the given input.
    /// Used for auto-detection of input language.
    fn can_handle(&self, input: &str) -> bool;
}

/// Adapter for Domain-Specific Languages that compile to our AST.
///
/// Unlike `QueryLanguageAdapter`, DSL adapters may maintain state
/// (e.g., variable bindings, session context) across invocations.
///
/// # Example
///
/// ```ignore
/// struct PythonDSLAdapter { runtime: PyRuntime }
///
/// impl DSLAdapter for PythonDSLAdapter {
///     fn name(&self) -> &str { "Python DataFrame DSL" }
///     fn compile_to_ast(&self, code: &str) -> Result<Query> {
///         // Evaluate Python code, extract query builder chain, produce AST
///     }
/// }
/// ```
pub trait DSLAdapter: Send + Sync {
    /// Human-readable name of the DSL.
    fn name(&self) -> &str;

    /// Compile DSL code into a single query AST.
    fn compile_to_ast(&self, code: &str) -> Result<Query>;

    /// Return supported file extensions for this DSL (e.g., `["py", "python"]`).
    fn file_extensions(&self) -> Vec<&str> {
        vec![]
    }
}

/// Built-in PostgreSQL adapter using our parser.
pub struct PostgresAdapter;

impl QueryLanguageAdapter for PostgresAdapter {
    fn name(&self) -> &str {
        "PostgreSQL"
    }

    fn parse(&self, input: &str) -> Result<Vec<Query>> {
        super::parser::parse_sql(input)
    }

    fn can_handle(&self, input: &str) -> bool {
        let trimmed = input.trim().to_uppercase();
        // Basic heuristic: starts with a SQL keyword
        trimmed.starts_with("SELECT")
            || trimmed.starts_with("INSERT")
            || trimmed.starts_with("UPDATE")
            || trimmed.starts_with("DELETE")
            || trimmed.starts_with("WITH")
            || trimmed.starts_with("CREATE")
            || trimmed.starts_with("ALTER")
            || trimmed.starts_with("DROP")
            || trimmed.starts_with("EXPLAIN")
            || trimmed.starts_with("SHOW")
    }
}

/// Registry for managing multiple language adapters.
pub struct AdapterRegistry {
    query_adapters: Vec<Box<dyn QueryLanguageAdapter>>,
    dsl_adapters: Vec<Box<dyn DSLAdapter>>,
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        let mut registry = Self {
            query_adapters: Vec::new(),
            dsl_adapters: Vec::new(),
        };
        // Register the built-in PostgreSQL adapter
        registry.register_query_adapter(Box::new(PostgresAdapter));
        registry
    }
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_query_adapter(&mut self, adapter: Box<dyn QueryLanguageAdapter>) {
        self.query_adapters.push(adapter);
    }

    pub fn register_dsl_adapter(&mut self, adapter: Box<dyn DSLAdapter>) {
        self.dsl_adapters.push(adapter);
    }

    /// Parse input using the first adapter that can handle it.
    pub fn parse(&self, input: &str) -> Result<Vec<Query>> {
        for adapter in &self.query_adapters {
            if adapter.can_handle(input) {
                return adapter.parse(input);
            }
        }
        anyhow::bail!("No adapter found that can handle this input")
    }

    /// List all registered adapter names.
    pub fn adapter_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.query_adapters.iter().map(|a| a.name()).collect();
        names.extend(self.dsl_adapters.iter().map(|a| a.name()));
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_postgres_adapter_can_handle() {
        let adapter = PostgresAdapter;
        assert!(adapter.can_handle("SELECT * FROM users"));
        assert!(adapter.can_handle("  select * from users  "));
        assert!(adapter.can_handle("INSERT INTO users VALUES (1)"));
        assert!(adapter.can_handle("WITH cte AS (SELECT 1) SELECT * FROM cte"));
        assert!(!adapter.can_handle("df.filter(col('x') > 1)"));
    }

    #[test]
    fn test_postgres_adapter_parse() {
        let adapter = PostgresAdapter;
        let queries = adapter.parse("SELECT * FROM users").unwrap();
        assert_eq!(queries.len(), 1);
    }

    #[test]
    fn test_registry_default_has_postgres() {
        let registry = AdapterRegistry::new();
        assert!(registry.adapter_names().contains(&"PostgreSQL"));
    }

    #[test]
    fn test_registry_parse_sql() {
        let registry = AdapterRegistry::new();
        let queries = registry.parse("SELECT 1").unwrap();
        assert_eq!(queries.len(), 1);
    }

    #[test]
    fn test_registry_no_adapter_for_unknown() {
        let registry = AdapterRegistry::new();
        let result = registry.parse("df.show()");
        assert!(result.is_err());
    }
}
