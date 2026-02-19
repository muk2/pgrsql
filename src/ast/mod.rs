/// Unified Query AST and Multi-Language Execution Engine.
///
/// This module provides the foundational architecture for pgrsql's
/// query processing pipeline:
///
/// ```text
/// Input (SQL / DSL)
///       ↓
/// Language Adapter Layer  (adapter.rs)
///       ↓
/// Unified Query AST       (types.rs)
///       ↓
/// Analysis / Optimization (optimizer.rs)
///       ↓
/// SQL Compiler            (compiler.rs)
///       ↓
/// Execution Engine        (existing db/ module)
/// ```
///
/// The plugin system (plugin.rs) allows external extensions to add
/// new adapters, optimization passes, and more.
pub mod adapter;
pub mod compiler;
pub mod formatter;
pub mod optimizer;
pub mod parser;
pub mod plugin;
pub mod types;

// Re-export key types for convenience
pub use adapter::{AdapterRegistry, DSLAdapter, QueryLanguageAdapter};
pub use compiler::compile;
pub use formatter::format_sql;
pub use optimizer::{analyze_query, OptimizationPass, Optimizer, QueryAnalysis};
pub use parser::{parse_single, parse_sql};
pub use plugin::{PluginRegistry, QueryPlugin};
pub use types::*;
