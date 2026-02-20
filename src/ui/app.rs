use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;
use tokio_postgres::Client;

use crate::db::{
    create_client, execute_query, get_columns, get_databases, get_indexes, get_schemas,
    get_table_ddl, get_tables, ColumnDetails, ConnectionConfig, ConnectionManager, DatabaseInfo,
    IndexInfo, QueryResult, SchemaInfo, SslMode, TableInfo,
};
use crate::editor::{HistoryEntry, QueryHistory, TextBuffer};
use crate::explain::{is_explain_query, parse_explain_output, QueryPlan};
use crate::ui::{Theme, SQL_KEYWORDS, SQL_TYPES};

pub const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Focus {
    Sidebar,
    Editor,
    Results,
    ConnectionDialog,
    Help,
    TableInspector,
    ExportPicker,
}

#[derive(Debug, Clone)]
pub struct TableInspectorState {
    pub table_name: String,
    pub schema_name: String,
    pub columns: Vec<ColumnDetails>,
    pub indexes: Vec<IndexInfo>,
    pub ddl: String,
    pub show_ddl: bool,
    pub scroll: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExportFormat {
    Csv,
    Json,
    SqlInsert,
    Tsv,
    ClipboardCsv,
}

pub const EXPORT_FORMATS: &[ExportFormat] = &[
    ExportFormat::Csv,
    ExportFormat::Json,
    ExportFormat::SqlInsert,
    ExportFormat::Tsv,
    ExportFormat::ClipboardCsv,
];

impl ExportFormat {
    pub fn label(&self) -> &'static str {
        match self {
            ExportFormat::Csv => "CSV (.csv)",
            ExportFormat::Json => "JSON (.json)",
            ExportFormat::SqlInsert => "SQL INSERT (.sql)",
            ExportFormat::Tsv => "TSV (.tsv)",
            ExportFormat::ClipboardCsv => "Copy to clipboard (CSV)",
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            ExportFormat::Csv => "csv",
            ExportFormat::Json => "json",
            ExportFormat::SqlInsert => "sql",
            ExportFormat::Tsv => "tsv",
            ExportFormat::ClipboardCsv => "csv",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SidebarTab {
    Databases,
    Tables,
    History,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum TreeNode {
    Database(DatabaseInfo),
    Schema(SchemaInfo),
    Table(TableInfo),
    Column(ColumnDetails),
}

#[allow(dead_code)]
pub struct App {
    pub theme: Theme,
    pub focus: Focus,
    pub should_quit: bool,

    // Connection
    pub connection: ConnectionManager,
    pub connection_dialog: ConnectionDialogState,

    // Sidebar
    pub sidebar_tab: SidebarTab,
    pub sidebar_width: u16,
    pub databases: Vec<DatabaseInfo>,
    pub schemas: Vec<SchemaInfo>,
    pub tables: Vec<TableInfo>,
    pub selected_table_columns: Vec<ColumnDetails>,
    pub sidebar_selected: usize,
    pub sidebar_scroll: usize,
    pub expanded_schemas: Vec<String>,
    pub expanded_tables: Vec<String>,

    // Editor
    pub editor: TextBuffer,
    pub query_history: QueryHistory,

    // Layout
    pub editor_height_percent: u16,

    // Results
    pub results: Vec<QueryResult>,
    pub current_result: usize,
    pub result_scroll_x: usize,
    pub result_scroll_y: usize,
    pub result_selected_row: usize,
    pub result_selected_col: usize,

    // Toasts
    pub toasts: Vec<Toast>,

    // Loading
    pub is_loading: bool,
    pub loading_message: String,
    pub spinner_frame: usize,

    // Help
    pub show_help: bool,

    // Autocomplete
    pub autocomplete: AutocompleteState,

    // EXPLAIN plan
    pub explain_plans: Vec<Option<QueryPlan>>,
    pub show_visual_plan: bool,
    pub plan_scroll: usize,

    // Table Inspector
    pub table_inspector: Option<TableInspectorState>,

    // Export
    pub export_selected: usize,

    // Async connection task
    pub pending_connection: Option<(ConnectionConfig, JoinHandle<Result<Client>>)>,
}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum StatusType {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Clone)]
pub struct Toast {
    pub message: String,
    pub status_type: StatusType,
    pub created_at: Instant,
    pub duration: Duration,
}

impl Toast {
    pub fn new(message: String, status_type: StatusType) -> Self {
        let duration = match status_type {
            StatusType::Info | StatusType::Success => Duration::from_secs(3),
            StatusType::Warning => Duration::from_secs(5),
            StatusType::Error => Duration::from_secs(8),
        };
        Self {
            message,
            status_type,
            created_at: Instant::now(),
            duration,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() >= self.duration
    }

    /// Returns progress from 0.0 (just created) to 1.0 (about to expire)
    pub fn progress(&self) -> f64 {
        let elapsed = self.created_at.elapsed().as_secs_f64();
        let total = self.duration.as_secs_f64();
        (elapsed / total).min(1.0)
    }
}

#[derive(Debug, Clone)]
pub struct ConnectionDialogState {
    pub active: bool,
    pub config: ConnectionConfig,
    pub field_index: usize,
    /// Cursor position within each text field (fields 0-5)
    pub field_cursors: [usize; 6],
    pub saved_connections: Vec<ConnectionConfig>,
    pub selected_saved: Option<usize>,
    /// Inline status message shown inside the dialog
    pub status_message: Option<(String, StatusType)>,
}

impl Default for ConnectionDialogState {
    fn default() -> Self {
        let config = ConnectionConfig::default();
        let field_cursors = [
            config.name.len(),
            config.host.len(),
            config.port.to_string().len(),
            config.database.len(),
            config.username.len(),
            config.password.len(),
        ];
        Self {
            active: false,
            config,
            field_index: 0,
            field_cursors,
            saved_connections: Vec::new(),
            selected_saved: None,
            status_message: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AutocompleteSuggestion {
    pub text: String,
    pub kind: SuggestionKind,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum SuggestionKind {
    Keyword,
    Type,
    Table,
    Column,
    Function,
}

impl SuggestionKind {
    pub fn label(self) -> &'static str {
        match self {
            SuggestionKind::Keyword => "KW",
            SuggestionKind::Type => "TY",
            SuggestionKind::Table => "TB",
            SuggestionKind::Column => "CL",
            SuggestionKind::Function => "FN",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AutocompleteState {
    pub active: bool,
    pub suggestions: Vec<AutocompleteSuggestion>,
    pub selected: usize,
    pub prefix: String,
}

pub const SQL_FUNCTIONS: &[&str] = &[
    "COUNT",
    "SUM",
    "AVG",
    "MIN",
    "MAX",
    "COALESCE",
    "NULLIF",
    "CAST",
    "NOW",
    "CURRENT_DATE",
    "CURRENT_TIMESTAMP",
    "EXTRACT",
    "DATE_TRUNC",
    "TO_CHAR",
    "TO_DATE",
    "TO_NUMBER",
    "TO_TIMESTAMP",
    "CONCAT",
    "LENGTH",
    "LOWER",
    "UPPER",
    "TRIM",
    "SUBSTRING",
    "REPLACE",
    "POSITION",
    "LEFT",
    "RIGHT",
    "LPAD",
    "RPAD",
    "SPLIT_PART",
    "STRING_AGG",
    "ARRAY_AGG",
    "JSON_AGG",
    "JSONB_AGG",
    "JSON_BUILD_OBJECT",
    "JSONB_BUILD_OBJECT",
    "ROW_NUMBER",
    "RANK",
    "DENSE_RANK",
    "LAG",
    "LEAD",
    "FIRST_VALUE",
    "LAST_VALUE",
    "NTILE",
    "GREATEST",
    "LEAST",
    "ABS",
    "CEIL",
    "FLOOR",
    "ROUND",
    "MOD",
    "POWER",
    "SQRT",
    "RANDOM",
    "GEN_RANDOM_UUID",
    "PG_SIZE_PRETTY",
    "PG_TOTAL_RELATION_SIZE",
    "PG_RELATION_SIZE",
];

impl App {
    pub fn new() -> Self {
        let query_history = QueryHistory::load().unwrap_or_default();
        let saved_connections = ConnectionManager::load_saved_connections().unwrap_or_default();

        // Try to auto-populate last used connection
        let last_connection_name = ConnectionManager::load_last_connection();
        let (initial_config, initial_field_index, initial_selected_saved) =
            if let Some(ref name) = last_connection_name {
                if let Some((idx, conn)) = saved_connections
                    .iter()
                    .enumerate()
                    .find(|(_, c)| &c.name == name)
                {
                    (conn.clone(), 5_usize, Some(idx)) // Focus on password field
                } else {
                    (ConnectionConfig::default(), 0_usize, None)
                }
            } else {
                (ConnectionConfig::default(), 0_usize, None)
            };

        let field_cursors = [
            initial_config.name.len(),
            initial_config.host.len(),
            initial_config.port.to_string().len(),
            initial_config.database.len(),
            initial_config.username.len(),
            initial_config.password.len(),
        ];

        Self {
            theme: Theme::dark(),
            focus: Focus::ConnectionDialog,
            should_quit: false,

            connection: ConnectionManager::new(),
            connection_dialog: ConnectionDialogState {
                active: true,
                config: initial_config,
                field_index: initial_field_index,
                field_cursors,
                saved_connections,
                selected_saved: initial_selected_saved,
                status_message: None,
            },

            sidebar_tab: SidebarTab::Tables,
            sidebar_width: 35,
            databases: Vec::new(),
            schemas: Vec::new(),
            tables: Vec::new(),
            selected_table_columns: Vec::new(),
            sidebar_selected: 0,
            sidebar_scroll: 0,
            expanded_schemas: vec!["public".to_string()],
            expanded_tables: Vec::new(),

            editor: TextBuffer::new(),
            query_history,

            editor_height_percent: 40,

            results: Vec::new(),
            current_result: 0,
            result_scroll_x: 0,
            result_scroll_y: 0,
            result_selected_row: 0,
            result_selected_col: 0,

            toasts: Vec::new(),
            is_loading: false,
            loading_message: String::new(),
            spinner_frame: 0,
            show_help: false,
            autocomplete: AutocompleteState::default(),

            explain_plans: Vec::new(),
            show_visual_plan: true,
            plan_scroll: 0,

            table_inspector: None,
            export_selected: 0,
            pending_connection: None,
        }
    }

    pub async fn try_auto_connect(&mut self, mut config: ConnectionConfig) {
        // Pre-fill the connection dialog with this config
        self.connection_dialog.config = config.clone();
        self.connection_dialog.field_cursors = [
            config.name.len(),
            config.host.len(),
            config.port.to_string().len(),
            config.database.len(),
            config.username.len(),
            config.password.len(),
        ];

        if config.database.trim().is_empty() {
            config.database = "postgres".to_string();
        }

        // Auto-connect blocks since the UI isn't running yet
        match self.connection.connect(config.clone()).await {
            Ok(()) if self.connection.is_connected() => {
                self.connection_dialog.active = false;
                self.focus = Focus::Editor;
                if !self
                    .connection_dialog
                    .saved_connections
                    .iter()
                    .any(|c| c.name == config.name)
                {
                    self.connection_dialog
                        .saved_connections
                        .push(config.clone());
                    let _ = ConnectionManager::save_connections(
                        &self.connection_dialog.saved_connections,
                    );
                }
                let _ = ConnectionManager::save_last_connection(&config.name);
                let _ = self.refresh_schema().await;
                self.set_status(
                    format!("Connected to {}", config.display_string()),
                    StatusType::Success,
                );
            }
            Ok(()) => {
                self.connection_dialog.active = true;
                self.focus = Focus::ConnectionDialog;
            }
            Err(e) => {
                self.connection_dialog.status_message =
                    Some((format!("Connection failed: {}", e), StatusType::Error));
                self.connection_dialog.active = true;
                self.focus = Focus::ConnectionDialog;
            }
        }
    }

    pub async fn handle_input(&mut self, key: KeyEvent) -> Result<()> {
        // Global shortcuts
        match (key.code, key.modifiers) {
            (KeyCode::Char('?'), _) if self.focus != Focus::Editor => {
                self.show_help = !self.show_help;
                if self.show_help {
                    self.focus = Focus::Help;
                } else {
                    self.focus = Focus::Editor;
                }
                return Ok(());
            }
            (KeyCode::Esc, _) if self.show_help => {
                self.show_help = false;
                self.focus = Focus::Editor;
                return Ok(());
            }
            _ => {}
        }

        match self.focus {
            Focus::ConnectionDialog => self.handle_connection_dialog_input(key).await,
            Focus::Sidebar => self.handle_sidebar_input(key).await,
            Focus::Editor => self.handle_editor_input(key).await,
            Focus::Results => self.handle_results_input(key).await,
            Focus::Help => self.handle_help_input(key).await,
            Focus::TableInspector => self.handle_table_inspector_input(key).await,
            Focus::ExportPicker => self.handle_export_input(key).await,
        }
    }

    async fn handle_connection_dialog_input(&mut self, key: KeyEvent) -> Result<()> {
        // Ignore input while connection is in progress (except Esc to cancel)
        if self.pending_connection.is_some() && key.code != KeyCode::Esc {
            return Ok(());
        }

        let dialog = &mut self.connection_dialog;

        match key.code {
            KeyCode::Esc => {
                // Cancel pending connection if any
                if let Some((_, handle)) = self.pending_connection.take() {
                    handle.abort();
                    self.stop_loading();
                    self.connection_dialog.status_message =
                        Some(("Connection cancelled".to_string(), StatusType::Warning));
                    return Ok(());
                }
                if self.connection.is_connected() {
                    dialog.active = false;
                    self.focus = Focus::Editor;
                } else {
                    // Quit when not connected
                    self.should_quit = true;
                }
            }
            KeyCode::Tab => {
                dialog.field_index = (dialog.field_index + 1) % 7;
            }
            KeyCode::BackTab => {
                dialog.field_index = if dialog.field_index == 0 {
                    6
                } else {
                    dialog.field_index - 1
                };
            }
            KeyCode::Up => {
                if let Some(selected) = dialog.selected_saved {
                    if selected > 0 {
                        dialog.selected_saved = Some(selected - 1);
                    }
                } else if !dialog.saved_connections.is_empty() {
                    dialog.selected_saved = Some(dialog.saved_connections.len() - 1);
                }
            }
            KeyCode::Down => {
                if let Some(selected) = dialog.selected_saved {
                    if selected < dialog.saved_connections.len() - 1 {
                        dialog.selected_saved = Some(selected + 1);
                    }
                } else if !dialog.saved_connections.is_empty() {
                    dialog.selected_saved = Some(0);
                }
            }
            KeyCode::Left => {
                if dialog.field_index == 6 {
                    // Cycle SSL mode backward
                    dialog.config.ssl_mode = match dialog.config.ssl_mode {
                        SslMode::Disable => SslMode::VerifyFull,
                        SslMode::Prefer => SslMode::Disable,
                        SslMode::Require => SslMode::Prefer,
                        SslMode::VerifyCa => SslMode::Require,
                        SslMode::VerifyFull => SslMode::VerifyCa,
                    };
                } else if dialog.field_cursors[dialog.field_index] > 0 {
                    dialog.field_cursors[dialog.field_index] -= 1;
                }
            }
            KeyCode::Right => {
                if dialog.field_index == 6 {
                    // Cycle SSL mode forward
                    dialog.config.ssl_mode = match dialog.config.ssl_mode {
                        SslMode::Disable => SslMode::Prefer,
                        SslMode::Prefer => SslMode::Require,
                        SslMode::Require => SslMode::VerifyCa,
                        SslMode::VerifyCa => SslMode::VerifyFull,
                        SslMode::VerifyFull => SslMode::Disable,
                    };
                } else {
                    let len = dialog_field_len(&dialog.config, dialog.field_index);
                    if dialog.field_cursors[dialog.field_index] < len {
                        dialog.field_cursors[dialog.field_index] += 1;
                    }
                }
            }
            KeyCode::Home => {
                if dialog.field_index < 6 {
                    dialog.field_cursors[dialog.field_index] = 0;
                }
            }
            KeyCode::End => {
                if dialog.field_index < 6 {
                    dialog.field_cursors[dialog.field_index] =
                        dialog_field_len(&dialog.config, dialog.field_index);
                }
            }
            KeyCode::Enter => {
                if let Some(idx) = dialog.selected_saved {
                    if idx < dialog.saved_connections.len() {
                        dialog.config = dialog.saved_connections[idx].clone();
                        dialog.field_cursors = [
                            dialog.config.name.len(),
                            dialog.config.host.len(),
                            dialog.config.port.to_string().len(),
                            dialog.config.database.len(),
                            dialog.config.username.len(),
                            dialog.config.password.len(),
                        ];
                        dialog.field_index = 5; // Auto-focus password field
                        dialog.selected_saved = None;
                    }
                } else {
                    self.start_connect();
                }
            }
            KeyCode::Char(c) => {
                if dialog.field_index == 6 {
                    return Ok(());
                }
                dialog.selected_saved = None;
                dialog.status_message = None;
                let cursor = dialog.field_cursors[dialog.field_index];
                match dialog.field_index {
                    0 => {
                        dialog.config.name.insert(cursor, c);
                        dialog.field_cursors[0] += 1;
                    }
                    1 => {
                        dialog.config.host.insert(cursor, c);
                        dialog.field_cursors[1] += 1;
                    }
                    2 => {
                        if c.is_ascii_digit() {
                            let mut port_str = dialog.config.port.to_string();
                            let pos = cursor.min(port_str.len());
                            port_str.insert(pos, c);
                            if let Ok(port) = port_str.parse::<u16>() {
                                dialog.config.port = port;
                                let new_len = dialog.config.port.to_string().len();
                                dialog.field_cursors[2] = (pos + 1).min(new_len);
                            }
                        }
                    }
                    3 => {
                        dialog.config.database.insert(cursor, c);
                        dialog.field_cursors[3] += 1;
                    }
                    4 => {
                        dialog.config.username.insert(cursor, c);
                        dialog.field_cursors[4] += 1;
                    }
                    5 => {
                        dialog.config.password.insert(cursor, c);
                        dialog.field_cursors[5] += 1;
                    }
                    _ => {}
                }
            }
            KeyCode::Backspace => {
                if dialog.field_index == 6 {
                    return Ok(());
                }
                dialog.selected_saved = None;
                let cursor = dialog.field_cursors[dialog.field_index];
                if cursor == 0 {
                    return Ok(());
                }
                match dialog.field_index {
                    0 => {
                        dialog.config.name.remove(cursor - 1);
                        dialog.field_cursors[0] -= 1;
                    }
                    1 => {
                        dialog.config.host.remove(cursor - 1);
                        dialog.field_cursors[1] -= 1;
                    }
                    2 => {
                        let mut port_str = dialog.config.port.to_string();
                        if cursor <= port_str.len() {
                            port_str.remove(cursor - 1);
                            dialog.config.port = if port_str.is_empty() {
                                0
                            } else {
                                port_str.parse().unwrap_or(0)
                            };
                            let new_len = dialog.config.port.to_string().len();
                            dialog.field_cursors[2] = (cursor - 1).min(new_len);
                        }
                    }
                    3 => {
                        dialog.config.database.remove(cursor - 1);
                        dialog.field_cursors[3] -= 1;
                    }
                    4 => {
                        dialog.config.username.remove(cursor - 1);
                        dialog.field_cursors[4] -= 1;
                    }
                    5 => {
                        dialog.config.password.remove(cursor - 1);
                        dialog.field_cursors[5] -= 1;
                    }
                    _ => {}
                }
            }
            KeyCode::Delete => {
                if let Some(idx) = dialog.selected_saved {
                    // Delete saved connection
                    if idx < dialog.saved_connections.len() {
                        dialog.saved_connections.remove(idx);
                        let _ = ConnectionManager::save_connections(&dialog.saved_connections);
                        if dialog.saved_connections.is_empty() {
                            dialog.selected_saved = None;
                        } else if idx >= dialog.saved_connections.len() {
                            dialog.selected_saved = Some(dialog.saved_connections.len() - 1);
                        }
                        self.set_status("Connection deleted".to_string(), StatusType::Info);
                    }
                } else {
                    // Delete character in text field
                    if dialog.field_index == 6 {
                        return Ok(());
                    }
                    dialog.selected_saved = None;
                    let cursor = dialog.field_cursors[dialog.field_index];
                    let len = dialog_field_len(&dialog.config, dialog.field_index);
                    if cursor >= len {
                        return Ok(());
                    }
                    match dialog.field_index {
                        0 => {
                            dialog.config.name.remove(cursor);
                        }
                        1 => {
                            dialog.config.host.remove(cursor);
                        }
                        2 => {
                            let mut port_str = dialog.config.port.to_string();
                            if cursor < port_str.len() {
                                port_str.remove(cursor);
                                dialog.config.port = if port_str.is_empty() {
                                    0
                                } else {
                                    port_str.parse().unwrap_or(0)
                                };
                                let new_len = dialog.config.port.to_string().len();
                                dialog.field_cursors[2] = cursor.min(new_len);
                            }
                        }
                        3 => {
                            dialog.config.database.remove(cursor);
                        }
                        4 => {
                            dialog.config.username.remove(cursor);
                        }
                        5 => {
                            dialog.config.password.remove(cursor);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_sidebar_input(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Tab | KeyCode::Right => {
                self.focus = Focus::Editor;
            }
            KeyCode::Up => {
                if self.sidebar_selected > 0 {
                    self.sidebar_selected -= 1;
                }
            }
            KeyCode::Down => {
                let max = match self.sidebar_tab {
                    SidebarTab::Databases => self.databases.len(),
                    SidebarTab::Tables => self.tables.len() + self.schemas.len(),
                    SidebarTab::History => self.query_history.entries().len(),
                };
                if self.sidebar_selected < max.saturating_sub(1) {
                    self.sidebar_selected += 1;
                }
            }
            KeyCode::Enter => {
                self.handle_sidebar_select().await?;
            }
            KeyCode::Char('1') => {
                self.sidebar_tab = SidebarTab::Databases;
                self.sidebar_selected = 0;
            }
            KeyCode::Char('2') => {
                self.sidebar_tab = SidebarTab::Tables;
                self.sidebar_selected = 0;
            }
            KeyCode::Char('3') => {
                self.sidebar_tab = SidebarTab::History;
                self.sidebar_selected = 0;
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.focus = Focus::ConnectionDialog;
                self.connection_dialog.active = true;
            }
            KeyCode::Char('i') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.open_table_inspector().await;
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_editor_input(&mut self, key: KeyEvent) -> Result<()> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);

        // Handle autocomplete navigation when active
        if self.autocomplete.active {
            match key.code {
                KeyCode::Tab | KeyCode::Enter => {
                    self.accept_autocomplete();
                    return Ok(());
                }
                KeyCode::Esc => {
                    self.autocomplete.active = false;
                    return Ok(());
                }
                KeyCode::Up => {
                    if self.autocomplete.selected > 0 {
                        self.autocomplete.selected -= 1;
                    }
                    return Ok(());
                }
                KeyCode::Down => {
                    if self.autocomplete.selected
                        < self.autocomplete.suggestions.len().saturating_sub(1)
                    {
                        self.autocomplete.selected += 1;
                    }
                    return Ok(());
                }
                _ => {
                    // Fall through to normal handling, but dismiss autocomplete for non-text keys
                    if !matches!(key.code, KeyCode::Char(_) | KeyCode::Backspace) {
                        self.autocomplete.active = false;
                    }
                }
            }
        }

        // Ctrl+Space triggers autocomplete
        if ctrl && key.code == KeyCode::Char(' ') {
            self.update_autocomplete();
            return Ok(());
        }

        match key.code {
            KeyCode::Tab if !ctrl => {
                if shift {
                    self.focus = Focus::Sidebar;
                } else {
                    self.editor.insert_tab();
                }
            }
            KeyCode::BackTab => {
                self.focus = Focus::Sidebar;
            }
            KeyCode::Enter if ctrl => {
                self.autocomplete.active = false;
                self.execute_query().await?;
                self.focus = Focus::Results;
            }
            KeyCode::F(5) => {
                self.autocomplete.active = false;
                self.execute_query().await?;
                self.focus = Focus::Results;
            }
            KeyCode::Enter => {
                self.editor.insert_newline();
                self.autocomplete.active = false;
            }
            KeyCode::Char('c') if ctrl => {
                self.editor.copy();
            }
            KeyCode::Char('x') if ctrl => {
                self.editor.cut();
            }
            KeyCode::Char('v') if ctrl => {
                self.editor.paste();
            }
            KeyCode::Char('a') if ctrl => {
                self.editor.select_all();
            }
            KeyCode::Char('z') if ctrl && shift => {
                self.editor.redo();
            }
            KeyCode::Char('z') if ctrl => {
                self.editor.undo();
            }
            KeyCode::Char('y') if ctrl => {
                self.editor.redo();
            }
            KeyCode::Char('l') if ctrl => {
                self.editor.clear();
                self.autocomplete.active = false;
            }
            // Pane resizing: Ctrl+Shift+Up/Down
            KeyCode::Up if ctrl && shift => {
                // Make editor smaller / results bigger
                if self.editor_height_percent > 15 {
                    self.editor_height_percent -= 5;
                }
            }
            KeyCode::Down if ctrl && shift => {
                // Make editor bigger / results smaller
                if self.editor_height_percent < 85 {
                    self.editor_height_percent += 5;
                }
            }
            // History navigation: Ctrl+Up/Down
            KeyCode::Up if ctrl => {
                if let Some(entry) = self.query_history.previous() {
                    self.editor.set_text(&entry.query);
                }
            }
            KeyCode::Down if ctrl => {
                if let Some(entry) = self.query_history.next() {
                    self.editor.set_text(&entry.query);
                }
            }
            KeyCode::Char(c) => {
                self.editor.insert_char(c);
                self.update_autocomplete();
            }
            KeyCode::Backspace => {
                self.editor.backspace();
                self.update_autocomplete();
            }
            KeyCode::Delete => {
                self.editor.delete();
            }
            KeyCode::Left if ctrl => {
                self.editor.move_word_left();
                self.autocomplete.active = false;
            }
            KeyCode::Right if ctrl => {
                self.editor.move_word_right();
                self.autocomplete.active = false;
            }
            KeyCode::Left => {
                self.editor.move_left();
                self.autocomplete.active = false;
            }
            KeyCode::Right => {
                self.editor.move_right();
                self.autocomplete.active = false;
            }
            KeyCode::Up => {
                self.editor.move_up();
                self.autocomplete.active = false;
            }
            KeyCode::Down => {
                self.editor.move_down();
                self.autocomplete.active = false;
            }
            KeyCode::Home if ctrl => {
                self.editor.move_to_start();
            }
            KeyCode::End if ctrl => {
                self.editor.move_to_end();
            }
            KeyCode::Home => {
                self.editor.move_to_line_start();
            }
            KeyCode::End => {
                self.editor.move_to_line_end();
            }
            KeyCode::Esc => {
                if self.editor.has_selection() {
                    self.editor.clear_selection();
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_results_input(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            // Tab/Shift+Tab for column navigation (Snowflake-style)
            KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => {
                // Shift+Tab: move to previous column
                if self.result_selected_col > 0 {
                    self.result_selected_col -= 1;
                }
            }
            KeyCode::BackTab => {
                // BackTab: move to previous column
                if self.result_selected_col > 0 {
                    self.result_selected_col -= 1;
                }
            }
            KeyCode::Tab => {
                // Tab: move to next column
                if let Some(result) = self.results.get(self.current_result) {
                    if self.result_selected_col < result.columns.len().saturating_sub(1) {
                        self.result_selected_col += 1;
                    }
                }
            }
            KeyCode::Esc => {
                // Esc to leave results and go back to editor
                self.focus = Focus::Editor;
            }
            KeyCode::Up if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.focus = Focus::Editor;
            }
            KeyCode::Left => {
                if self.result_selected_col > 0 {
                    self.result_selected_col -= 1;
                }
            }
            KeyCode::Right => {
                if let Some(result) = self.results.get(self.current_result) {
                    if self.result_selected_col < result.columns.len().saturating_sub(1) {
                        self.result_selected_col += 1;
                    }
                }
            }
            KeyCode::Up => {
                if self.result_selected_row > 0 {
                    self.result_selected_row -= 1;
                    self.auto_scroll_results();
                }
            }
            KeyCode::Down => {
                if let Some(result) = self.results.get(self.current_result) {
                    if self.result_selected_row < result.rows.len().saturating_sub(1) {
                        self.result_selected_row += 1;
                        self.auto_scroll_results();
                    }
                }
            }
            KeyCode::Home => {
                self.result_selected_col = 0;
            }
            KeyCode::End => {
                if let Some(result) = self.results.get(self.current_result) {
                    self.result_selected_col = result.columns.len().saturating_sub(1);
                }
            }
            KeyCode::PageUp => {
                self.result_selected_row = self.result_selected_row.saturating_sub(20);
                self.auto_scroll_results();
            }
            KeyCode::PageDown => {
                if let Some(result) = self.results.get(self.current_result) {
                    self.result_selected_row =
                        (self.result_selected_row + 20).min(result.rows.len().saturating_sub(1));
                    self.auto_scroll_results();
                }
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.copy_selected_cell();
            }
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if !self.results.is_empty() {
                    self.export_selected = 0;
                    self.focus = Focus::ExportPicker;
                }
            }
            KeyCode::Char('[') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.current_result > 0 {
                    self.current_result -= 1;
                    self.result_selected_row = 0;
                    self.result_selected_col = 0;
                    self.result_scroll_y = 0;
                }
            }
            KeyCode::Char(']') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.current_result < self.results.len().saturating_sub(1) {
                    self.current_result += 1;
                    self.result_selected_row = 0;
                    self.result_selected_col = 0;
                    self.result_scroll_y = 0;
                }
            }
            KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Toggle between visual plan and raw text for EXPLAIN results
                if self
                    .explain_plans
                    .get(self.current_result)
                    .and_then(|p| p.as_ref())
                    .is_some()
                {
                    self.show_visual_plan = !self.show_visual_plan;
                    self.plan_scroll = 0;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Keep the selected result row visible by adjusting scroll position.
    fn auto_scroll_results(&mut self) {
        if self.result_selected_row < self.result_scroll_y {
            self.result_scroll_y = self.result_selected_row;
        }
        // Use a conservative visible-height estimate; rendering will clamp if needed
        let estimated_visible = 20_usize;
        if self.result_selected_row >= self.result_scroll_y + estimated_visible {
            self.result_scroll_y = self
                .result_selected_row
                .saturating_sub(estimated_visible - 1);
        }
    }

    async fn handle_help_input(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => {
                self.show_help = false;
                self.focus = Focus::Editor;
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_table_inspector_input(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.table_inspector = None;
                self.focus = Focus::Sidebar;
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                if let Some(ref mut inspector) = self.table_inspector {
                    inspector.show_ddl = !inspector.show_ddl;
                    inspector.scroll = 0;
                }
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(ref inspector) = self.table_inspector {
                    if inspector.show_ddl {
                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            let _ = clipboard.set_text(&inspector.ddl);
                            self.set_status(
                                "DDL copied to clipboard".to_string(),
                                StatusType::Success,
                            );
                        }
                    }
                }
            }
            KeyCode::Up => {
                if let Some(ref mut inspector) = self.table_inspector {
                    inspector.scroll = inspector.scroll.saturating_sub(1);
                }
            }
            KeyCode::Down => {
                if let Some(ref mut inspector) = self.table_inspector {
                    inspector.scroll += 1;
                }
            }
            KeyCode::PageUp => {
                if let Some(ref mut inspector) = self.table_inspector {
                    inspector.scroll = inspector.scroll.saturating_sub(10);
                }
            }
            KeyCode::PageDown => {
                if let Some(ref mut inspector) = self.table_inspector {
                    inspector.scroll += 10;
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_export_input(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.focus = Focus::Results;
            }
            KeyCode::Up => {
                if self.export_selected > 0 {
                    self.export_selected -= 1;
                }
            }
            KeyCode::Down => {
                if self.export_selected < EXPORT_FORMATS.len() - 1 {
                    self.export_selected += 1;
                }
            }
            KeyCode::Enter => {
                let format = EXPORT_FORMATS[self.export_selected];
                self.perform_export(format);
                self.focus = Focus::Results;
            }
            KeyCode::Char(c @ '1'..='5') => {
                let idx = (c as usize) - ('1' as usize);
                if idx < EXPORT_FORMATS.len() {
                    let format = EXPORT_FORMATS[idx];
                    self.perform_export(format);
                    self.focus = Focus::Results;
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn open_table_inspector(&mut self) {
        if self.sidebar_tab != SidebarTab::Tables || self.connection.client.is_none() {
            return;
        }

        // Find the selected table from the sidebar
        let mut index = 0;
        let mut target_table: Option<(String, String)> = None;

        for schema in &self.schemas {
            if index == self.sidebar_selected {
                // Schema is selected, not a table
                return;
            }
            index += 1;

            if self.expanded_schemas.contains(&schema.name) {
                for table in &self.tables {
                    if table.schema == schema.name {
                        if index == self.sidebar_selected {
                            target_table = Some((schema.name.clone(), table.name.clone()));
                            break;
                        }
                        index += 1;
                    }
                }
                if target_table.is_some() {
                    break;
                }
            }
        }

        let (schema_name, table_name) = match target_table {
            Some(t) => t,
            None => return,
        };

        let client = self.connection.client.as_ref().unwrap();

        let columns = get_columns(client, &schema_name, &table_name)
            .await
            .unwrap_or_default();
        let indexes = get_indexes(client, &schema_name, &table_name)
            .await
            .unwrap_or_default();
        let ddl = get_table_ddl(client, &schema_name, &table_name)
            .await
            .unwrap_or_else(|_| "-- DDL generation failed".to_string());

        self.table_inspector = Some(TableInspectorState {
            table_name,
            schema_name,
            columns,
            indexes,
            ddl,
            show_ddl: false,
            scroll: 0,
        });
        self.focus = Focus::TableInspector;
    }

    fn perform_export(&mut self, format: ExportFormat) {
        let result = match self.results.get(self.current_result) {
            Some(r) => r,
            None => {
                self.set_status("No results to export".to_string(), StatusType::Warning);
                return;
            }
        };

        let content = match format {
            ExportFormat::Csv => crate::export::to_csv(result),
            ExportFormat::Json => crate::export::to_json(result),
            ExportFormat::SqlInsert => crate::export::to_sql_insert(result, "results"),
            ExportFormat::Tsv => crate::export::to_tsv(result),
            ExportFormat::ClipboardCsv => {
                let csv = crate::export::to_csv(result);
                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    let _ = clipboard.set_text(&csv);
                    self.set_status(
                        format!("Copied {} rows to clipboard", result.row_count),
                        StatusType::Success,
                    );
                } else {
                    self.set_status("Failed to access clipboard".to_string(), StatusType::Error);
                }
                return;
            }
        };

        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let filename = format!("pgrsql_export_{}.{}", timestamp, format.extension());

        match std::fs::write(&filename, &content) {
            Ok(()) => {
                self.set_status(
                    format!("Exported {} rows to {}", result.row_count, filename),
                    StatusType::Success,
                );
            }
            Err(e) => {
                self.set_status(format!("Export failed: {}", e), StatusType::Error);
            }
        }
    }

    async fn handle_sidebar_select(&mut self) -> Result<()> {
        match self.sidebar_tab {
            SidebarTab::Databases => {
                if let Some(db) = self.databases.get(self.sidebar_selected) {
                    let db_name = db.name.clone();
                    self.connection.switch_database(&db_name).await?;
                    self.refresh_schema().await?;
                    self.set_status(
                        format!("Switched to database: {}", db_name),
                        StatusType::Success,
                    );
                }
            }
            SidebarTab::Tables => {
                // Calculate if it's a schema or table
                let mut index = 0;
                for schema in &self.schemas {
                    if index == self.sidebar_selected {
                        // Toggle schema expansion
                        if self.expanded_schemas.contains(&schema.name) {
                            self.expanded_schemas.retain(|s| s != &schema.name);
                        } else {
                            self.expanded_schemas.push(schema.name.clone());
                        }
                        return Ok(());
                    }
                    index += 1;

                    if self.expanded_schemas.contains(&schema.name) {
                        for table in &self.tables {
                            if table.schema == schema.name {
                                if index == self.sidebar_selected {
                                    // Insert table name into editor
                                    let full_name = format!("{}.{}", table.schema, table.name);
                                    self.editor.insert_text(&full_name);
                                    self.focus = Focus::Editor;
                                    return Ok(());
                                }
                                index += 1;
                            }
                        }
                    }
                }
            }
            SidebarTab::History => {
                let entries = self.query_history.entries();
                if let Some(entry) = entries.get(entries.len() - 1 - self.sidebar_selected) {
                    self.editor.set_text(&entry.query);
                    self.focus = Focus::Editor;
                }
            }
        }
        Ok(())
    }

    fn start_connect(&mut self) {
        let mut config = self.connection_dialog.config.clone();

        if config.host.is_empty() || config.username.is_empty() {
            self.connection_dialog.status_message = Some((
                "Host and username are required".to_string(),
                StatusType::Error,
            ));
            return;
        }

        // Default database to "postgres" if left empty
        if config.database.trim().is_empty() {
            config.database = "postgres".to_string();
            self.connection_dialog.config.database = "postgres".to_string();
        }

        // Don't start another connection if one is already in progress
        if self.pending_connection.is_some() {
            return;
        }

        self.connection_dialog.status_message = Some((
            format!("Connecting to {}...", config.display_string()),
            StatusType::Info,
        ));
        self.start_loading(format!("Connecting to {}...", config.display_string()));

        let config_for_task = config.clone();
        let handle = tokio::spawn(async move { create_client(&config_for_task).await });
        self.pending_connection = Some((config, handle));
    }

    async fn finish_connect(&mut self, config: ConnectionConfig, client: Client) -> Result<()> {
        self.connection.apply_client(config.clone(), client);
        self.stop_loading();
        self.connection_dialog.status_message = None;
        self.connection_dialog.active = false;
        self.focus = Focus::Editor;

        // Save connection (without password)
        if !self
            .connection_dialog
            .saved_connections
            .iter()
            .any(|c| c.name == config.name)
        {
            self.connection_dialog
                .saved_connections
                .push(config.clone());
            let _ = ConnectionManager::save_connections(&self.connection_dialog.saved_connections);
        }

        // Save as last used connection
        let _ = ConnectionManager::save_last_connection(&config.name);

        self.refresh_schema().await?;
        self.set_status(
            format!("Connected to {}", config.display_string()),
            StatusType::Success,
        );
        Ok(())
    }

    async fn refresh_schema(&mut self) -> Result<()> {
        if self.connection.client.is_some() {
            self.start_loading("Loading schema...".to_string());

            let client = self.connection.client.as_ref().unwrap();

            let db_result = get_databases(client).await;
            let schema_result = get_schemas(client).await;

            let schemas_for_tables = match &schema_result {
                Ok(s) => s.clone(),
                Err(_) => Vec::new(),
            };
            let mut all_tables = Vec::new();
            for schema in &schemas_for_tables {
                if let Ok(tables) = get_tables(client, &schema.name).await {
                    all_tables.extend(tables);
                }
            }

            // All client usage is done above; now we can mutably borrow self
            let databases = match db_result {
                Ok(dbs) => dbs,
                Err(e) => {
                    self.set_status(
                        format!("Failed to load databases: {}", e),
                        StatusType::Warning,
                    );
                    Vec::new()
                }
            };
            let schemas = match schema_result {
                Ok(s) => s,
                Err(e) => {
                    self.set_status(
                        format!("Failed to load schemas: {}", e),
                        StatusType::Warning,
                    );
                    Vec::new()
                }
            };

            self.databases = databases;
            self.schemas = schemas;
            self.tables = all_tables;
            self.stop_loading();
        }
        Ok(())
    }

    /// Get the byte offset of the cursor in the full editor text.
    fn get_cursor_offset(&self) -> usize {
        let mut offset = 0;
        for (i, line) in self.editor.lines.iter().enumerate() {
            if i == self.editor.cursor_y {
                offset += self.editor.cursor_x;
                break;
            }
            offset += line.len() + 1; // +1 for newline
        }
        offset
    }

    /// Find the query at the current cursor position.
    /// Splits on `;` while respecting string literals and comments.
    fn get_query_at_cursor(&self) -> String {
        let full_text = self.editor.text();
        let cursor_offset = self.get_cursor_offset();

        let boundaries = Self::find_query_boundaries(&full_text);
        for (start, end) in &boundaries {
            if cursor_offset >= *start && cursor_offset <= *end {
                return full_text[*start..*end].trim().to_string();
            }
        }

        // Fallback to full text
        full_text.trim().to_string()
    }

    /// Returns (start_line, end_line) of the query block at the cursor,
    /// for visual highlighting in the editor.
    pub fn get_current_query_line_range(&self) -> Option<(usize, usize)> {
        let full_text = self.editor.text();
        let cursor_offset = self.get_cursor_offset();

        let boundaries = Self::find_query_boundaries(&full_text);
        for (start, end) in &boundaries {
            if cursor_offset >= *start && cursor_offset <= *end {
                // Convert byte offsets to line numbers
                let start_line = full_text[..*start].matches('\n').count();
                let end_line = full_text[..*end].matches('\n').count();
                return Some((start_line, end_line));
            }
        }
        None
    }

    /// Find all query boundaries in the text, returning (start, end) byte offsets.
    /// Respects single-quoted strings, double-quoted identifiers, and `--` line comments.
    fn find_query_boundaries(text: &str) -> Vec<(usize, usize)> {
        let mut boundaries = Vec::new();
        let mut start = 0;
        let mut in_single_quote = false;
        let mut in_double_quote = false;
        let mut in_line_comment = false;
        let mut in_block_comment = false;
        let chars: Vec<char> = text.chars().collect();
        let len = chars.len();
        let mut byte_pos = 0;
        let mut i = 0;

        while i < len {
            let c = chars[i];
            let c_len = c.len_utf8();

            if in_line_comment {
                if c == '\n' {
                    in_line_comment = false;
                }
            } else if in_block_comment {
                if c == '*' && i + 1 < len && chars[i + 1] == '/' {
                    in_block_comment = false;
                    i += 1;
                    byte_pos += chars[i].len_utf8();
                }
            } else if in_single_quote {
                if c == '\'' {
                    // Handle escaped quotes ('')
                    if i + 1 < len && chars[i + 1] == '\'' {
                        i += 1;
                        byte_pos += chars[i].len_utf8();
                    } else {
                        in_single_quote = false;
                    }
                }
            } else if in_double_quote {
                if c == '"' {
                    in_double_quote = false;
                }
            } else {
                match c {
                    '\'' => in_single_quote = true,
                    '"' => in_double_quote = true,
                    '-' if i + 1 < len && chars[i + 1] == '-' => {
                        in_line_comment = true;
                    }
                    '/' if i + 1 < len && chars[i + 1] == '*' => {
                        in_block_comment = true;
                        i += 1;
                        byte_pos += chars[i].len_utf8();
                    }
                    ';' => {
                        let end = byte_pos;
                        if !text[start..end].trim().is_empty() {
                            boundaries.push((start, end));
                        }
                        start = byte_pos + c_len;
                    }
                    _ => {}
                }
            }

            byte_pos += c_len;
            i += 1;
        }

        // Last query (after final `;` or if no `;` at all)
        if start < text.len() && !text[start..].trim().is_empty() {
            boundaries.push((start, text.len()));
        }

        // If empty, treat entire text as one query
        if boundaries.is_empty() && !text.trim().is_empty() {
            boundaries.push((0, text.len()));
        }

        boundaries
    }

    async fn execute_query(&mut self) -> Result<()> {
        let query = self.get_query_at_cursor();
        if query.trim().is_empty() {
            return Ok(());
        }

        if self.connection.client.is_some() {
            self.start_loading("Executing query...".to_string());

            let client = self.connection.client.as_ref().unwrap();
            let result = execute_query(client, &query).await?;
            self.stop_loading();

            // Add to history
            let entry = HistoryEntry {
                query: query.clone(),
                timestamp: chrono::Utc::now(),
                database: self.connection.current_database.clone(),
                execution_time_ms: result.execution_time.as_millis() as u64,
                success: result.error.is_none(),
            };
            self.query_history.add(entry);
            let _ = self.query_history.save();

            // Update status
            if let Some(err) = &result.error {
                self.set_status(format!("{}: {}", err.category, err.message), StatusType::Error);
            } else if let Some(affected) = result.affected_rows {
                self.set_status(
                    format!(
                        "{} rows affected ({:.2}ms)",
                        affected,
                        result.execution_time.as_secs_f64() * 1000.0
                    ),
                    StatusType::Success,
                );
            } else {
                self.set_status(
                    format!(
                        "{} rows returned ({:.2}ms)",
                        result.row_count,
                        result.execution_time.as_secs_f64() * 1000.0
                    ),
                    StatusType::Success,
                );
            }

            // Parse EXPLAIN plan if applicable
            let plan = if is_explain_query(&query) {
                // Build the text output from the result rows
                let text: String = result
                    .rows
                    .iter()
                    .filter_map(|row| row.first().map(|cell| cell.display()))
                    .collect::<Vec<String>>()
                    .join("\n");
                parse_explain_output(&text)
            } else {
                None
            };

            self.results.push(result);
            self.explain_plans.push(plan);
            self.current_result = self.results.len() - 1;
            self.result_selected_row = 0;
            self.result_selected_col = 0;
            self.plan_scroll = 0;
            self.show_visual_plan = self
                .explain_plans
                .last()
                .map(|p| p.is_some())
                .unwrap_or(false);
        } else {
            self.set_status("Not connected to database".to_string(), StatusType::Error);
        }

        Ok(())
    }

    fn update_autocomplete(&mut self) {
        let line = self.editor.current_line().to_string();
        let cursor_x = self.editor.cursor_x;

        // Extract the word being typed (prefix), including dots for schema.table
        let before_cursor = &line[..cursor_x.min(line.len())];
        let prefix_start = before_cursor
            .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
            .map(|i| i + 1)
            .unwrap_or(0);
        let prefix = &before_cursor[prefix_start..];

        if prefix.len() < 2 {
            self.autocomplete.active = false;
            return;
        }

        let prefix_upper = prefix.to_uppercase();
        let prefix_lower = prefix.to_lowercase();

        let mut suggestions: Vec<AutocompleteSuggestion> = Vec::new();

        // Table names from loaded schema (schema-qualified)
        let mut seen_tables = std::collections::HashSet::new();
        for table in &self.tables {
            let qualified = format!("{}.{}", table.schema, table.name);
            if seen_tables.contains(&qualified) {
                continue;
            }
            // Match on bare table name OR schema.table qualified name
            if table.name.to_lowercase().starts_with(&prefix_lower)
                || qualified.to_lowercase().starts_with(&prefix_lower)
            {
                suggestions.push(AutocompleteSuggestion {
                    text: qualified.clone(),
                    kind: SuggestionKind::Table,
                });
                seen_tables.insert(qualified);
            }
        }

        // SQL keywords
        for &kw in SQL_KEYWORDS {
            if kw.starts_with(&prefix_upper) {
                suggestions.push(AutocompleteSuggestion {
                    text: kw.to_string(),
                    kind: SuggestionKind::Keyword,
                });
            }
        }

        // SQL types
        for &ty in SQL_TYPES {
            if ty.starts_with(&prefix_upper) {
                suggestions.push(AutocompleteSuggestion {
                    text: ty.to_string(),
                    kind: SuggestionKind::Type,
                });
            }
        }

        // SQL functions
        for &func in SQL_FUNCTIONS {
            if func.starts_with(&prefix_upper) {
                suggestions.push(AutocompleteSuggestion {
                    text: format!("{}()", func),
                    kind: SuggestionKind::Function,
                });
            }
        }

        // Limit to 10 suggestions
        suggestions.truncate(10);

        if suggestions.is_empty() {
            self.autocomplete.active = false;
        } else {
            self.autocomplete.active = true;
            self.autocomplete.suggestions = suggestions;
            self.autocomplete.selected = 0;
            self.autocomplete.prefix = prefix.to_string();
        }
    }

    fn accept_autocomplete(&mut self) {
        if let Some(suggestion) = self
            .autocomplete
            .suggestions
            .get(self.autocomplete.selected)
        {
            let text = suggestion.text.clone();
            let prefix_len = self.autocomplete.prefix.len();

            // Delete the prefix
            for _ in 0..prefix_len {
                self.editor.backspace();
            }

            // Insert the suggestion
            self.editor.insert_text(&text);
        }
        self.autocomplete.active = false;
    }

    fn copy_selected_cell(&mut self) {
        if let Some(result) = self.results.get(self.current_result) {
            if let Some(row) = result.rows.get(self.result_selected_row) {
                if let Some(cell) = row.get(self.result_selected_col) {
                    let text = cell.display();
                    if let Ok(mut clipboard) = arboard::Clipboard::new() {
                        let _ = clipboard.set_text(&text);
                        self.set_status("Cell copied to clipboard".to_string(), StatusType::Info);
                    }
                }
            }
        }
    }

    fn set_status(&mut self, message: String, status_type: StatusType) {
        let toast = Toast::new(message, status_type);
        self.toasts.push(toast);
        // Keep max 5 toasts
        if self.toasts.len() > 5 {
            self.toasts.remove(0);
        }
    }

    fn start_loading(&mut self, message: String) {
        self.is_loading = true;
        self.loading_message = message;
    }

    fn stop_loading(&mut self) {
        self.is_loading = false;
        self.loading_message.clear();
    }

    pub async fn tick(&mut self) -> Result<()> {
        // Remove expired toasts
        self.toasts.retain(|t| !t.is_expired());

        // Advance spinner frame when loading
        if self.is_loading {
            self.spinner_frame = (self.spinner_frame + 1) % SPINNER_FRAMES.len();
        }

        // Poll pending connection task
        if let Some((_, handle)) = &self.pending_connection {
            if handle.is_finished() {
                let (config, handle) = self.pending_connection.take().unwrap();
                match handle.await {
                    Ok(Ok(client)) => {
                        self.finish_connect(config, client).await?;
                    }
                    Ok(Err(e)) => {
                        self.stop_loading();
                        let msg = format!("Connection failed: {}", e);
                        self.connection_dialog.status_message =
                            Some((msg.clone(), StatusType::Error));
                        self.set_status(msg, StatusType::Error);
                    }
                    Err(e) => {
                        self.stop_loading();
                        let msg = format!("Connection task failed: {}", e);
                        self.connection_dialog.status_message =
                            Some((msg.clone(), StatusType::Error));
                        self.set_status(msg, StatusType::Error);
                    }
                }
            }
        }

        Ok(())
    }
}

fn dialog_field_len(config: &ConnectionConfig, field_index: usize) -> usize {
    match field_index {
        0 => config.name.len(),
        1 => config.host.len(),
        2 => config.port.to_string().len(),
        3 => config.database.len(),
        4 => config.username.len(),
        5 => config.password.len(),
        _ => 0,
    }
}
