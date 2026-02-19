/// Plugin architecture for extending pgrsql.
///
/// Plugins can register new query languages, optimization passes,
/// custom SQL functions, and execution strategies. This provides
/// a clean extension point without modifying core code.
use anyhow::Result;

use super::adapter::{DSLAdapter, QueryLanguageAdapter};
use super::optimizer::OptimizationPass;

/// Trait that all pgrsql plugins must implement.
///
/// A plugin registers its capabilities with the `PluginRegistry`
/// during initialization. Plugins are loaded and initialized once
/// at startup.
///
/// # Example
///
/// ```ignore
/// struct MyPlugin;
///
/// impl QueryPlugin for MyPlugin {
///     fn name(&self) -> &str { "my-plugin" }
///     fn version(&self) -> &str { "0.1.0" }
///     fn register(&self, registry: &mut PluginRegistry) -> Result<()> {
///         registry.add_optimization_pass(Box::new(MyOptPass));
///         Ok(())
///     }
/// }
/// ```
pub trait QueryPlugin: Send + Sync {
    /// Unique plugin identifier.
    fn name(&self) -> &str;

    /// Plugin version string.
    fn version(&self) -> &str;

    /// Optional description.
    fn description(&self) -> &str {
        ""
    }

    /// Register plugin capabilities with the registry.
    fn register(&self, registry: &mut PluginRegistry) -> Result<()>;
}

/// Central registry for all plugin-provided capabilities.
#[derive(Default)]
pub struct PluginRegistry {
    query_adapters: Vec<Box<dyn QueryLanguageAdapter>>,
    dsl_adapters: Vec<Box<dyn DSLAdapter>>,
    optimization_passes: Vec<Box<dyn OptimizationPass>>,
    loaded_plugins: Vec<PluginInfo>,
}

#[derive(Debug, Clone)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    pub description: String,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a query language adapter.
    pub fn add_query_adapter(&mut self, adapter: Box<dyn QueryLanguageAdapter>) {
        self.query_adapters.push(adapter);
    }

    /// Register a DSL adapter.
    pub fn add_dsl_adapter(&mut self, adapter: Box<dyn DSLAdapter>) {
        self.dsl_adapters.push(adapter);
    }

    /// Register an optimization pass.
    pub fn add_optimization_pass(&mut self, pass: Box<dyn OptimizationPass>) {
        self.optimization_passes.push(pass);
    }

    /// Load and initialize a plugin.
    pub fn load_plugin(&mut self, plugin: Box<dyn QueryPlugin>) -> Result<()> {
        let info = PluginInfo {
            name: plugin.name().to_string(),
            version: plugin.version().to_string(),
            description: plugin.description().to_string(),
        };

        plugin.register(self)?;
        self.loaded_plugins.push(info);
        Ok(())
    }

    /// Get all registered query adapters.
    pub fn query_adapters(&self) -> &[Box<dyn QueryLanguageAdapter>] {
        &self.query_adapters
    }

    /// Get all registered DSL adapters.
    pub fn dsl_adapters(&self) -> &[Box<dyn DSLAdapter>] {
        &self.dsl_adapters
    }

    /// Take ownership of all optimization passes (for building an optimizer).
    pub fn take_optimization_passes(&mut self) -> Vec<Box<dyn OptimizationPass>> {
        std::mem::take(&mut self.optimization_passes)
    }

    /// List loaded plugins.
    pub fn loaded_plugins(&self) -> &[PluginInfo] {
        &self.loaded_plugins
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestPlugin;

    impl QueryPlugin for TestPlugin {
        fn name(&self) -> &str {
            "test-plugin"
        }

        fn version(&self) -> &str {
            "0.1.0"
        }

        fn description(&self) -> &str {
            "A test plugin"
        }

        fn register(&self, _registry: &mut PluginRegistry) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_registry_empty() {
        let registry = PluginRegistry::new();
        assert!(registry.loaded_plugins().is_empty());
        assert!(registry.query_adapters().is_empty());
    }

    #[test]
    fn test_load_plugin() {
        let mut registry = PluginRegistry::new();
        registry.load_plugin(Box::new(TestPlugin)).unwrap();
        assert_eq!(registry.loaded_plugins().len(), 1);
        assert_eq!(registry.loaded_plugins()[0].name, "test-plugin");
        assert_eq!(registry.loaded_plugins()[0].version, "0.1.0");
    }

    #[test]
    fn test_take_optimization_passes() {
        let mut registry = PluginRegistry::new();
        // Initially empty
        let passes = registry.take_optimization_passes();
        assert!(passes.is_empty());
    }
}
