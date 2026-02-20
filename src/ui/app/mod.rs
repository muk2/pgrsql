mod connection;
mod editor;
mod results;
mod sidebar;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;
use tokio_postgres::Client;

use crate::db::{
    ColumnDetails, ConnectionConfig, ConnectionManager, DatabaseInfo, IndexInfo, QueryResult,
    SchemaInfo, TableInfo,
};
use crate::editor::{QueryHistory, TextBuffer};
use crate::explain::QueryPlan;
use crate::ui::Theme;

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

    pub(crate) fn set_status(&mut self, message: String, status_type: StatusType) {
        let toast = Toast::new(message, status_type);
        self.toasts.push(toast);
        // Keep max 5 toasts
        if self.toasts.len() > 5 {
            self.toasts.remove(0);
        }
    }

    pub(crate) fn start_loading(&mut self, message: String) {
        self.is_loading = true;
        self.loading_message = message;
    }

    pub(crate) fn stop_loading(&mut self) {
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
