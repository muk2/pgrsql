use anyhow::{Context, Result};
use postgres_native_tls::MakeTlsConnector;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use tokio_postgres::{Client, NoTls};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    #[serde(skip_serializing, default)]
    pub password: String,
    pub ssl_mode: SslMode,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
pub enum SslMode {
    Disable,
    #[default]
    Prefer,
    Require,
}

impl ConnectionConfig {
    pub fn connection_string(&self) -> String {
        let sslmode = match self.ssl_mode {
            SslMode::Disable => "disable",
            SslMode::Prefer => "prefer",
            SslMode::Require => "require",
        };
        format!(
            "host={} port={} dbname={} user={} password={} sslmode={} connect_timeout=10",
            quote_conn_value(&self.host),
            self.port,
            quote_conn_value(&self.database),
            quote_conn_value(&self.username),
            quote_conn_value(&self.password),
            sslmode
        )
    }

    pub fn display_string(&self) -> String {
        format!(
            "{}@{}:{}/{}",
            self.username, self.host, self.port, self.database
        )
    }
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            name: String::from("Local PostgreSQL"),
            host: String::from("localhost"),
            port: 5432,
            database: String::from("postgres"),
            username: String::from("postgres"),
            password: String::new(),
            ssl_mode: SslMode::default(),
        }
    }
}

pub struct ConnectionManager {
    pub config: ConnectionConfig,
    pub client: Option<Client>,
    pub current_database: String,
    pub current_schema: String,
}

#[allow(dead_code)]
impl ConnectionManager {
    pub fn new() -> Self {
        Self {
            config: ConnectionConfig::default(),
            client: None,
            current_database: String::from("postgres"),
            current_schema: String::from("public"),
        }
    }

    pub fn apply_client(&mut self, config: ConnectionConfig, client: Client) {
        self.current_database = config.database.clone();
        self.config = config;
        self.client = Some(client);
    }

    pub async fn connect(&mut self, config: ConnectionConfig) -> Result<()> {
        let client = create_client(&config).await?;
        self.apply_client(config, client);
        Ok(())
    }

    pub async fn disconnect(&mut self) {
        self.client = None;
    }

    pub fn is_connected(&self) -> bool {
        self.client.is_some()
    }

    pub async fn switch_database(&mut self, database: &str) -> Result<()> {
        let mut new_config = self.config.clone();
        new_config.database = database.to_string();
        self.disconnect().await;
        self.connect(new_config).await
    }

    pub async fn switch_schema(&mut self, schema: &str) -> Result<()> {
        if let Some(client) = &self.client {
            client
                .execute(&format!("SET search_path TO {}", schema), &[])
                .await?;
            self.current_schema = schema.to_string();
        }
        Ok(())
    }

    pub fn get_config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("pgrsql")
            .join("connections.toml")
    }

    pub fn load_saved_connections() -> Result<Vec<ConnectionConfig>> {
        let path = Self::get_config_path();
        if !path.exists() {
            return Ok(vec![]);
        }
        let content = std::fs::read_to_string(&path)?;
        let connections: SavedConnections = toml::from_str(&content)?;
        Ok(connections.connections)
    }

    pub fn save_connections(connections: &[ConnectionConfig]) -> Result<()> {
        let path = Self::get_config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let saved = SavedConnections {
            connections: connections.to_vec(),
        };
        let content = toml::to_string_pretty(&saved)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    pub fn save_last_connection(name: &str) -> Result<()> {
        let path = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("pgrsql")
            .join("last_connection");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, name)?;
        Ok(())
    }

    pub fn load_last_connection() -> Option<String> {
        let path = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("pgrsql")
            .join("last_connection");
        std::fs::read_to_string(&path)
            .ok()
            .map(|s| s.trim().to_string())
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct SavedConnections {
    connections: Vec<ConnectionConfig>,
}

/// Create a PostgreSQL client without needing a ConnectionManager.
/// This is `Send` so it can be used with `tokio::spawn`.
pub async fn create_client(config: &ConnectionConfig) -> Result<Client> {
    let conn_string = config.connection_string();
    let timeout = Duration::from_secs(15);

    let client = match config.ssl_mode {
        SslMode::Disable => {
            let (client, connection) =
                tokio::time::timeout(timeout, tokio_postgres::connect(&conn_string, NoTls))
                    .await
                    .map_err(|_| anyhow::anyhow!("Connection timed out after 15s"))?
                    .context("Failed to connect to PostgreSQL")?;
            tokio::spawn(async move {
                if let Err(e) = connection.await {
                    eprintln!("Connection error: {}", e);
                }
            });
            client
        }
        SslMode::Prefer | SslMode::Require => {
            let connector = native_tls::TlsConnector::builder()
                .build()
                .context("Failed to build TLS connector")?;
            let tls = MakeTlsConnector::new(connector);
            let (client, connection) =
                tokio::time::timeout(timeout, tokio_postgres::connect(&conn_string, tls))
                    .await
                    .map_err(|_| anyhow::anyhow!("Connection timed out after 15s"))?
                    .context("Failed to connect to PostgreSQL")?;
            tokio::spawn(async move {
                if let Err(e) = connection.await {
                    eprintln!("Connection error: {}", e);
                }
            });
            client
        }
    };

    Ok(client)
}

/// Quote a value for use in a libpq key=value connection string.
/// Wraps in single quotes and escapes backslashes and single quotes.
fn quote_conn_value(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('\'', "\\'");
    format!("'{}'", escaped)
}
