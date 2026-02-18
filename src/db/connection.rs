use anyhow::{Context, Result};
use postgres_native_tls::MakeTlsConnector;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use tokio_postgres::{Client, NoTls};

/// AWS RDS root certificate bundle (global-bundle.pem)
/// Contains all AWS RDS Certificate Authority certificates for all regions.
/// This allows connections to any AWS RDS instance without requiring users
/// to manually download certificates.
/// Source: https://truststore.pki.rds.amazonaws.com/global/global-bundle.pem
const AWS_RDS_CA_BUNDLE: &[u8] = include_bytes!("aws-rds-global-bundle.pem");

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
    /// Accept invalid/self-signed certificates. Use with caution.
    /// When true, certificate verification is skipped (useful for testing or
    /// when using Require mode without strict verification).
    #[serde(default)]
    pub accept_invalid_certs: bool,
    /// Optional path to a custom CA certificate file (PEM format).
    /// If not set, uses system CA store or embedded AWS RDS certificates.
    #[serde(default)]
    pub ca_cert_path: Option<String>,
    /// Use embedded AWS RDS root certificates for verification.
    /// This is automatically enabled when connecting to *.rds.amazonaws.com hosts.
    #[serde(default)]
    pub use_aws_rds_certs: bool,
}

/// SSL/TLS connection modes for PostgreSQL.
///
/// These match the standard PostgreSQL sslmode parameter:
/// - `Disable`: No SSL (unencrypted)
/// - `Prefer`: Try SSL first, fall back to non-SSL (default)
/// - `Require`: Require SSL but don't verify certificate
/// - `VerifyCa`: Require SSL and verify the server certificate is signed by a trusted CA
/// - `VerifyFull`: Like VerifyCa, but also verify the server hostname matches the certificate
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
pub enum SslMode {
    Disable,
    #[default]
    Prefer,
    Require,
    VerifyCa,
    VerifyFull,
}

impl ConnectionConfig {
    pub fn connection_string(&self) -> String {
        let sslmode = match self.ssl_mode {
            SslMode::Disable => "disable",
            SslMode::Prefer => "prefer",
            SslMode::Require => "require",
            SslMode::VerifyCa => "verify-ca",
            SslMode::VerifyFull => "verify-full",
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

    /// Check if the host looks like an AWS RDS endpoint.
    pub fn is_aws_rds_host(&self) -> bool {
        self.host.contains(".rds.amazonaws.com")
            || self.host.contains(".rds.cn-")
            || self.host.contains(".rds-fips.")
    }

    /// Determine if we should use AWS RDS certificates.
    /// Returns true if explicitly enabled or if host looks like AWS RDS.
    pub fn should_use_aws_rds_certs(&self) -> bool {
        self.use_aws_rds_certs || self.is_aws_rds_host()
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
            accept_invalid_certs: false,
            ca_cert_path: None,
            use_aws_rds_certs: false,
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
            // For Prefer/Require: use TLS but certificate verification depends on settings
            let tls = build_tls_connector(config, false)?;
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
        SslMode::VerifyCa | SslMode::VerifyFull => {
            // For VerifyCa/VerifyFull: strict certificate verification
            let tls = build_tls_connector(config, true)?;
            let (client, connection) =
                tokio::time::timeout(timeout, tokio_postgres::connect(&conn_string, tls))
                    .await
                    .map_err(|_| anyhow::anyhow!("Connection timed out after 15s"))?
                    .context("Failed to connect to PostgreSQL with certificate verification")?;
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

/// Build a TLS connector with appropriate certificate configuration.
///
/// # Arguments
/// * `config` - Connection configuration
/// * `strict_verify` - If true, always verify certificates (for verify-ca/verify-full modes)
fn build_tls_connector(config: &ConnectionConfig, strict_verify: bool) -> Result<MakeTlsConnector> {
    let mut builder = native_tls::TlsConnector::builder();

    // Handle certificate verification
    if config.accept_invalid_certs && !strict_verify {
        // User explicitly wants to skip verification (only for Prefer/Require modes)
        builder.danger_accept_invalid_certs(true);
        builder.danger_accept_invalid_hostnames(true);
    } else {
        // Load CA certificates
        if let Some(ca_path) = &config.ca_cert_path {
            // User provided a custom CA certificate file
            let ca_data = std::fs::read(ca_path)
                .with_context(|| format!("Failed to read CA certificate file: {}", ca_path))?;
            add_ca_certificates(&mut builder, &ca_data)?;
        } else if config.should_use_aws_rds_certs() {
            // Use embedded AWS RDS certificates
            add_ca_certificates(&mut builder, AWS_RDS_CA_BUNDLE)?;
        }
        // If neither custom CA nor AWS RDS certs, use system defaults
    }

    let connector = builder.build().context("Failed to build TLS connector")?;

    Ok(MakeTlsConnector::new(connector))
}

/// Add CA certificates from PEM data to the TLS builder.
fn add_ca_certificates(
    builder: &mut native_tls::TlsConnectorBuilder,
    pem_data: &[u8],
) -> Result<()> {
    // Parse all certificates from the PEM bundle
    let certs = parse_pem_certificates(pem_data)?;

    for cert_der in certs {
        let cert =
            native_tls::Certificate::from_der(&cert_der).context("Failed to parse certificate")?;
        builder.add_root_certificate(cert);
    }

    Ok(())
}

/// Parse PEM-encoded certificates and return DER-encoded data.
fn parse_pem_certificates(pem_data: &[u8]) -> Result<Vec<Vec<u8>>> {
    let pem_str =
        std::str::from_utf8(pem_data).context("CA certificate file is not valid UTF-8")?;

    let mut certs = Vec::new();
    let mut current_cert = String::new();
    let mut in_cert = false;

    for line in pem_str.lines() {
        if line.contains("-----BEGIN CERTIFICATE-----") {
            in_cert = true;
            current_cert.clear();
        } else if line.contains("-----END CERTIFICATE-----") {
            in_cert = false;
            if !current_cert.is_empty() {
                // Decode base64
                let der =
                    base64_decode(&current_cert).context("Failed to decode certificate base64")?;
                certs.push(der);
            }
        } else if in_cert {
            current_cert.push_str(line.trim());
        }
    }

    if certs.is_empty() {
        anyhow::bail!("No valid certificates found in PEM data");
    }

    Ok(certs)
}

/// Simple base64 decoder (avoids adding another dependency).
fn base64_decode(input: &str) -> Result<Vec<u8>> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    fn char_to_val(c: u8) -> Option<u8> {
        ALPHABET.iter().position(|&x| x == c).map(|p| p as u8)
    }

    let input: Vec<u8> = input
        .bytes()
        .filter(|&c| c != b'\n' && c != b'\r' && c != b' ')
        .collect();
    let mut output = Vec::with_capacity(input.len() * 3 / 4);

    for chunk in input.chunks(4) {
        let mut buf = [0u8; 4];
        let mut valid = 0;

        for (i, &c) in chunk.iter().enumerate() {
            if c == b'=' {
                break;
            }
            buf[i] = char_to_val(c).ok_or_else(|| anyhow::anyhow!("Invalid base64 character"))?;
            valid += 1;
        }

        if valid >= 2 {
            output.push((buf[0] << 2) | (buf[1] >> 4));
        }
        if valid >= 3 {
            output.push((buf[1] << 4) | (buf[2] >> 2));
        }
        if valid >= 4 {
            output.push((buf[2] << 6) | buf[3]);
        }
    }

    Ok(output)
}

/// Quote a value for use in a libpq key=value connection string.
/// Wraps in single quotes and escapes backslashes and single quotes.
fn quote_conn_value(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('\'', "\\'");
    format!("'{}'", escaped)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- ConnectionConfig ---

    #[test]
    fn test_default_config() {
        let config = ConnectionConfig::default();
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 5432);
        assert_eq!(config.database, "postgres");
        assert_eq!(config.username, "postgres");
        assert!(config.password.is_empty());
        assert_eq!(config.ssl_mode, SslMode::Prefer);
        assert!(!config.accept_invalid_certs);
        assert!(config.ca_cert_path.is_none());
        assert!(!config.use_aws_rds_certs);
    }

    #[test]
    fn test_connection_string() {
        let config = ConnectionConfig {
            host: "myhost".into(),
            port: 5433,
            database: "mydb".into(),
            username: "myuser".into(),
            password: "mypass".into(),
            ssl_mode: SslMode::Require,
            ..Default::default()
        };
        let s = config.connection_string();
        assert!(s.contains("host='myhost'"));
        assert!(s.contains("port=5433"));
        assert!(s.contains("dbname='mydb'"));
        assert!(s.contains("user='myuser'"));
        assert!(s.contains("password='mypass'"));
        assert!(s.contains("sslmode=require"));
    }

    #[test]
    fn test_connection_string_ssl_modes() {
        let modes = vec![
            (SslMode::Disable, "disable"),
            (SslMode::Prefer, "prefer"),
            (SslMode::Require, "require"),
            (SslMode::VerifyCa, "verify-ca"),
            (SslMode::VerifyFull, "verify-full"),
        ];
        for (mode, expected) in modes {
            let config = ConnectionConfig {
                ssl_mode: mode,
                ..Default::default()
            };
            assert!(
                config
                    .connection_string()
                    .contains(&format!("sslmode={}", expected)),
                "Expected sslmode={} for {:?}",
                expected,
                mode
            );
        }
    }

    #[test]
    fn test_display_string() {
        let config = ConnectionConfig {
            username: "admin".into(),
            host: "db.example.com".into(),
            port: 5432,
            database: "production".into(),
            ..Default::default()
        };
        assert_eq!(
            config.display_string(),
            "admin@db.example.com:5432/production"
        );
    }

    #[test]
    fn test_connection_string_escaping() {
        let config = ConnectionConfig {
            password: "pass'word\\test".into(),
            ..Default::default()
        };
        let s = config.connection_string();
        assert!(s.contains("pass\\'word\\\\test"));
    }

    // --- AWS RDS detection ---

    #[test]
    fn test_is_aws_rds_host() {
        let config = ConnectionConfig {
            host: "mydb.abc123.us-east-1.rds.amazonaws.com".into(),
            ..Default::default()
        };
        assert!(config.is_aws_rds_host());
    }

    #[test]
    fn test_is_not_aws_rds_host() {
        let config = ConnectionConfig {
            host: "localhost".into(),
            ..Default::default()
        };
        assert!(!config.is_aws_rds_host());
    }

    #[test]
    fn test_should_use_aws_rds_certs_auto() {
        let config = ConnectionConfig {
            host: "mydb.abc123.us-east-1.rds.amazonaws.com".into(),
            ..Default::default()
        };
        assert!(config.should_use_aws_rds_certs());
    }

    #[test]
    fn test_should_use_aws_rds_certs_explicit() {
        let config = ConnectionConfig {
            host: "custom-proxy.example.com".into(),
            use_aws_rds_certs: true,
            ..Default::default()
        };
        assert!(config.should_use_aws_rds_certs());
    }

    // --- ConnectionManager ---

    #[test]
    fn test_new_manager() {
        let mgr = ConnectionManager::new();
        assert!(!mgr.is_connected());
        assert_eq!(mgr.current_database, "postgres");
        assert_eq!(mgr.current_schema, "public");
    }

    // --- SSL mode default ---

    #[test]
    fn test_ssl_mode_default() {
        let mode = SslMode::default();
        assert_eq!(mode, SslMode::Prefer);
    }

    // --- quote_conn_value ---

    #[test]
    fn test_quote_simple() {
        assert_eq!(quote_conn_value("hello"), "'hello'");
    }

    #[test]
    fn test_quote_with_special_chars() {
        assert_eq!(quote_conn_value("it's"), "'it\\'s'");
        assert_eq!(quote_conn_value("back\\slash"), "'back\\\\slash'");
    }

    // --- base64 decoder ---

    #[test]
    fn test_base64_decode() {
        let decoded = base64_decode("SGVsbG8gV29ybGQ=").unwrap();
        assert_eq!(String::from_utf8(decoded).unwrap(), "Hello World");
    }

    #[test]
    fn test_base64_decode_no_padding() {
        let decoded = base64_decode("SGVsbG8").unwrap();
        assert_eq!(String::from_utf8(decoded).unwrap(), "Hello");
    }

    #[test]
    fn test_base64_decode_with_newlines() {
        let decoded = base64_decode("SGVs\nbG8=").unwrap();
        assert_eq!(String::from_utf8(decoded).unwrap(), "Hello");
    }

    // --- Serialization ---

    #[test]
    fn test_config_serialization() {
        let config = ConnectionConfig::default();
        let toml_str = toml::to_string(&config).unwrap();
        assert!(toml_str.contains("host"));
        assert!(!toml_str.contains("password")); // password is skip_serializing
    }

    #[test]
    fn test_config_deserialization() {
        let toml_str = r#"
            name = "Test"
            host = "localhost"
            port = 5432
            database = "testdb"
            username = "user"
            ssl_mode = "Require"
        "#;
        let config: ConnectionConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.name, "Test");
        assert_eq!(config.ssl_mode, SslMode::Require);
        assert!(config.password.is_empty());
    }
}
