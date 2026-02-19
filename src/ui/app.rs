use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;
use tokio_postgres::Client;

use crate::db::{
    create_client, execute_query, get_databases, get_schemas, get_tables, ColumnDetails,
    ConnectionConfig, ConnectionManager, DatabaseInfo, QueryResult, SchemaInfo, SslMode, TableInfo,
};
use crate::editor::{HistoryEntry, QueryHistory, TextBuffer};
use crate::ui::Theme;

pub const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Focus {
    Sidebar,
    Editor,
    Results,
    ConnectionDialog,
    Help,
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

/// Per-tab state: each tab has its own editor, connection, and results.
#[allow(dead_code)]
pub struct EditorTab {
    pub label: String,

    // Connection
    pub connection: ConnectionManager,

    // Sidebar
    pub sidebar_tab: SidebarTab,
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

    // Results
    pub results: Vec<QueryResult>,
    pub current_result: usize,
    pub result_scroll_x: usize,
    pub result_scroll_y: usize,
    pub result_selected_row: usize,
    pub result_selected_col: usize,

    // Async connection task
    pub pending_connection: Option<(ConnectionConfig, JoinHandle<Result<Client>>)>,
}

impl EditorTab {
    pub fn new() -> Self {
        let query_history = QueryHistory::load().unwrap_or_default();
        Self {
            label: "New tab".to_string(),
            connection: ConnectionManager::new(),
            sidebar_tab: SidebarTab::Tables,
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
            results: Vec::new(),
            current_result: 0,
            result_scroll_x: 0,
            result_scroll_y: 0,
            result_selected_row: 0,
            result_selected_col: 0,
            pending_connection: None,
        }
    }

    /// Display name for the tab bar
    pub fn display_name(&self) -> String {
        if self.connection.is_connected() {
            let db = &self.connection.current_database;
            if db.is_empty() {
                self.connection.config.name.clone()
            } else {
                db.clone()
            }
        } else {
            self.label.clone()
        }
    }
}

#[allow(dead_code)]
pub struct App {
    pub theme: Theme,
    pub focus: Focus,
    pub should_quit: bool,

    // Tabs
    pub tabs: Vec<EditorTab>,
    pub active_tab: usize,

    // Connection dialog (shared)
    pub connection_dialog: ConnectionDialogState,

    // Sidebar width (shared across tabs)
    pub sidebar_width: u16,

    // Toasts
    pub toasts: Vec<Toast>,

    // Loading
    pub is_loading: bool,
    pub loading_message: String,
    pub spinner_frame: usize,

    // Help
    pub show_help: bool,
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

impl App {
    pub fn new() -> Self {
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

        let first_tab = EditorTab::new();

        Self {
            theme: Theme::dark(),
            focus: Focus::ConnectionDialog,
            should_quit: false,

            tabs: vec![first_tab],
            active_tab: 0,

            connection_dialog: ConnectionDialogState {
                active: true,
                config: initial_config,
                field_index: initial_field_index,
                field_cursors,
                saved_connections,
                selected_saved: initial_selected_saved,
                status_message: None,
            },

            sidebar_width: 35,

            toasts: Vec::new(),
            is_loading: false,
            loading_message: String::new(),
            spinner_frame: 0,
            show_help: false,
        }
    }

    /// Get a reference to the active tab
    pub fn tab(&self) -> &EditorTab {
        &self.tabs[self.active_tab]
    }

    /// Get a mutable reference to the active tab
    pub fn tab_mut(&mut self) -> &mut EditorTab {
        &mut self.tabs[self.active_tab]
    }

    /// Create a new tab and switch to it
    pub fn new_tab(&mut self) {
        let tab = EditorTab::new();
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
        // Open connection dialog for new tab
        self.connection_dialog.active = true;
        self.focus = Focus::ConnectionDialog;
        self.set_status("New tab created".to_string(), StatusType::Info);
    }

    /// Close the active tab
    pub fn close_tab(&mut self) {
        if self.tabs.len() <= 1 {
            // Last tab: replace with a fresh one
            self.tabs[0] = EditorTab::new();
            self.active_tab = 0;
            self.connection_dialog.active = true;
            self.focus = Focus::ConnectionDialog;
            return;
        }
        self.tabs.remove(self.active_tab);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
    }

    /// Switch to next tab
    pub fn next_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active_tab = (self.active_tab + 1) % self.tabs.len();
        }
    }

    /// Switch to previous tab
    pub fn prev_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active_tab = if self.active_tab == 0 {
                self.tabs.len() - 1
            } else {
                self.active_tab - 1
            };
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
        match self.tab_mut().connection.connect(config.clone()).await {
            Ok(()) if self.tab().connection.is_connected() => {
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
            // Tab management shortcuts (global, not in connection dialog)
            (KeyCode::Char('t'), m)
                if m.contains(KeyModifiers::CONTROL) && self.focus != Focus::ConnectionDialog =>
            {
                self.new_tab();
                return Ok(());
            }
            (KeyCode::Char('w'), m)
                if m.contains(KeyModifiers::CONTROL) && self.focus != Focus::ConnectionDialog =>
            {
                self.close_tab();
                return Ok(());
            }
            // Alt+1..9 to switch tabs
            (KeyCode::Char(c @ '1'..='9'), m)
                if m.contains(KeyModifiers::ALT) && self.focus != Focus::ConnectionDialog =>
            {
                let idx = (c as usize) - ('1' as usize);
                if idx < self.tabs.len() {
                    self.active_tab = idx;
                }
                return Ok(());
            }
            // Alt+Right/Left to switch tabs
            (KeyCode::Right, m)
                if m.contains(KeyModifiers::ALT) && self.focus != Focus::ConnectionDialog =>
            {
                self.next_tab();
                return Ok(());
            }
            (KeyCode::Left, m)
                if m.contains(KeyModifiers::ALT) && self.focus != Focus::ConnectionDialog =>
            {
                self.prev_tab();
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
        }
    }

    async fn handle_connection_dialog_input(&mut self, key: KeyEvent) -> Result<()> {
        // Ignore input while connection is in progress (except Esc to cancel)
        if self.tab().pending_connection.is_some() && key.code != KeyCode::Esc {
            return Ok(());
        }

        match key.code {
            KeyCode::Esc => {
                // Cancel pending connection if any
                if let Some((_, handle)) = self.tab_mut().pending_connection.take() {
                    handle.abort();
                    self.stop_loading();
                    self.connection_dialog.status_message =
                        Some(("Connection cancelled".to_string(), StatusType::Warning));
                    return Ok(());
                }
                if self.tab().connection.is_connected() {
                    self.connection_dialog.active = false;
                    self.focus = Focus::Editor;
                } else if self.tabs.len() > 1 {
                    // Close this empty tab and switch back
                    self.close_tab();
                    self.focus = Focus::Editor;
                } else {
                    // Quit when not connected and it's the only tab
                    self.should_quit = true;
                }
            }
            KeyCode::Tab => {
                self.connection_dialog.field_index = (self.connection_dialog.field_index + 1) % 7;
            }
            KeyCode::BackTab => {
                self.connection_dialog.field_index = if self.connection_dialog.field_index == 0 {
                    6
                } else {
                    self.connection_dialog.field_index - 1
                };
            }
            KeyCode::Up => {
                if let Some(selected) = self.connection_dialog.selected_saved {
                    if selected > 0 {
                        self.connection_dialog.selected_saved = Some(selected - 1);
                    }
                } else if !self.connection_dialog.saved_connections.is_empty() {
                    self.connection_dialog.selected_saved =
                        Some(self.connection_dialog.saved_connections.len() - 1);
                }
            }
            KeyCode::Down => {
                if let Some(selected) = self.connection_dialog.selected_saved {
                    if selected < self.connection_dialog.saved_connections.len() - 1 {
                        self.connection_dialog.selected_saved = Some(selected + 1);
                    }
                } else if !self.connection_dialog.saved_connections.is_empty() {
                    self.connection_dialog.selected_saved = Some(0);
                }
            }
            KeyCode::Left => {
                if self.connection_dialog.field_index == 6 {
                    self.connection_dialog.config.ssl_mode =
                        match self.connection_dialog.config.ssl_mode {
                            SslMode::Disable => SslMode::VerifyFull,
                            SslMode::Prefer => SslMode::Disable,
                            SslMode::Require => SslMode::Prefer,
                            SslMode::VerifyCa => SslMode::Require,
                            SslMode::VerifyFull => SslMode::VerifyCa,
                        };
                } else {
                    let fi = self.connection_dialog.field_index;
                    if self.connection_dialog.field_cursors[fi] > 0 {
                        self.connection_dialog.field_cursors[fi] -= 1;
                    }
                }
            }
            KeyCode::Right => {
                if self.connection_dialog.field_index == 6 {
                    self.connection_dialog.config.ssl_mode =
                        match self.connection_dialog.config.ssl_mode {
                            SslMode::Disable => SslMode::Prefer,
                            SslMode::Prefer => SslMode::Require,
                            SslMode::Require => SslMode::VerifyCa,
                            SslMode::VerifyCa => SslMode::VerifyFull,
                            SslMode::VerifyFull => SslMode::Disable,
                        };
                } else {
                    let fi = self.connection_dialog.field_index;
                    let len = dialog_field_len(&self.connection_dialog.config, fi);
                    if self.connection_dialog.field_cursors[fi] < len {
                        self.connection_dialog.field_cursors[fi] += 1;
                    }
                }
            }
            KeyCode::Home => {
                let fi = self.connection_dialog.field_index;
                if fi < 6 {
                    self.connection_dialog.field_cursors[fi] = 0;
                }
            }
            KeyCode::End => {
                let fi = self.connection_dialog.field_index;
                if fi < 6 {
                    self.connection_dialog.field_cursors[fi] =
                        dialog_field_len(&self.connection_dialog.config, fi);
                }
            }
            KeyCode::Enter => {
                if let Some(idx) = self.connection_dialog.selected_saved {
                    if idx < self.connection_dialog.saved_connections.len() {
                        self.connection_dialog.config =
                            self.connection_dialog.saved_connections[idx].clone();
                        self.connection_dialog.field_cursors = [
                            self.connection_dialog.config.name.len(),
                            self.connection_dialog.config.host.len(),
                            self.connection_dialog.config.port.to_string().len(),
                            self.connection_dialog.config.database.len(),
                            self.connection_dialog.config.username.len(),
                            self.connection_dialog.config.password.len(),
                        ];
                        self.connection_dialog.field_index = 5;
                        self.connection_dialog.selected_saved = None;
                    }
                } else {
                    self.start_connect();
                }
            }
            KeyCode::Char(c) => {
                let fi = self.connection_dialog.field_index;
                if fi == 6 {
                    return Ok(());
                }
                self.connection_dialog.selected_saved = None;
                self.connection_dialog.status_message = None;
                let cursor = self.connection_dialog.field_cursors[fi];
                match fi {
                    0 => {
                        self.connection_dialog.config.name.insert(cursor, c);
                        self.connection_dialog.field_cursors[0] += 1;
                    }
                    1 => {
                        self.connection_dialog.config.host.insert(cursor, c);
                        self.connection_dialog.field_cursors[1] += 1;
                    }
                    2 => {
                        if c.is_ascii_digit() {
                            let mut port_str = self.connection_dialog.config.port.to_string();
                            let pos = cursor.min(port_str.len());
                            port_str.insert(pos, c);
                            if let Ok(port) = port_str.parse::<u16>() {
                                self.connection_dialog.config.port = port;
                                let new_len = self.connection_dialog.config.port.to_string().len();
                                self.connection_dialog.field_cursors[2] = (pos + 1).min(new_len);
                            }
                        }
                    }
                    3 => {
                        self.connection_dialog.config.database.insert(cursor, c);
                        self.connection_dialog.field_cursors[3] += 1;
                    }
                    4 => {
                        self.connection_dialog.config.username.insert(cursor, c);
                        self.connection_dialog.field_cursors[4] += 1;
                    }
                    5 => {
                        self.connection_dialog.config.password.insert(cursor, c);
                        self.connection_dialog.field_cursors[5] += 1;
                    }
                    _ => {}
                }
            }
            KeyCode::Backspace => {
                let fi = self.connection_dialog.field_index;
                if fi == 6 {
                    return Ok(());
                }
                self.connection_dialog.selected_saved = None;
                let cursor = self.connection_dialog.field_cursors[fi];
                if cursor == 0 {
                    return Ok(());
                }
                match fi {
                    0 => {
                        self.connection_dialog.config.name.remove(cursor - 1);
                        self.connection_dialog.field_cursors[0] -= 1;
                    }
                    1 => {
                        self.connection_dialog.config.host.remove(cursor - 1);
                        self.connection_dialog.field_cursors[1] -= 1;
                    }
                    2 => {
                        let mut port_str = self.connection_dialog.config.port.to_string();
                        if cursor <= port_str.len() {
                            port_str.remove(cursor - 1);
                            self.connection_dialog.config.port = if port_str.is_empty() {
                                0
                            } else {
                                port_str.parse().unwrap_or(0)
                            };
                            let new_len = self.connection_dialog.config.port.to_string().len();
                            self.connection_dialog.field_cursors[2] = (cursor - 1).min(new_len);
                        }
                    }
                    3 => {
                        self.connection_dialog.config.database.remove(cursor - 1);
                        self.connection_dialog.field_cursors[3] -= 1;
                    }
                    4 => {
                        self.connection_dialog.config.username.remove(cursor - 1);
                        self.connection_dialog.field_cursors[4] -= 1;
                    }
                    5 => {
                        self.connection_dialog.config.password.remove(cursor - 1);
                        self.connection_dialog.field_cursors[5] -= 1;
                    }
                    _ => {}
                }
            }
            KeyCode::Delete => {
                if let Some(idx) = self.connection_dialog.selected_saved {
                    if idx < self.connection_dialog.saved_connections.len() {
                        self.connection_dialog.saved_connections.remove(idx);
                        let _ = ConnectionManager::save_connections(
                            &self.connection_dialog.saved_connections,
                        );
                        if self.connection_dialog.saved_connections.is_empty() {
                            self.connection_dialog.selected_saved = None;
                        } else if idx >= self.connection_dialog.saved_connections.len() {
                            self.connection_dialog.selected_saved =
                                Some(self.connection_dialog.saved_connections.len() - 1);
                        }
                        self.set_status("Connection deleted".to_string(), StatusType::Info);
                    }
                } else {
                    let fi = self.connection_dialog.field_index;
                    if fi == 6 {
                        return Ok(());
                    }
                    self.connection_dialog.selected_saved = None;
                    let cursor = self.connection_dialog.field_cursors[fi];
                    let len = dialog_field_len(&self.connection_dialog.config, fi);
                    if cursor >= len {
                        return Ok(());
                    }
                    match fi {
                        0 => {
                            self.connection_dialog.config.name.remove(cursor);
                        }
                        1 => {
                            self.connection_dialog.config.host.remove(cursor);
                        }
                        2 => {
                            let mut port_str = self.connection_dialog.config.port.to_string();
                            if cursor < port_str.len() {
                                port_str.remove(cursor);
                                self.connection_dialog.config.port = if port_str.is_empty() {
                                    0
                                } else {
                                    port_str.parse().unwrap_or(0)
                                };
                                let new_len = self.connection_dialog.config.port.to_string().len();
                                self.connection_dialog.field_cursors[2] = cursor.min(new_len);
                            }
                        }
                        3 => {
                            self.connection_dialog.config.database.remove(cursor);
                        }
                        4 => {
                            self.connection_dialog.config.username.remove(cursor);
                        }
                        5 => {
                            self.connection_dialog.config.password.remove(cursor);
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
                let tab = self.tab_mut();
                if tab.sidebar_selected > 0 {
                    tab.sidebar_selected -= 1;
                }
            }
            KeyCode::Down => {
                let tab = self.tab();
                let max = match tab.sidebar_tab {
                    SidebarTab::Databases => tab.databases.len(),
                    SidebarTab::Tables => tab.tables.len() + tab.schemas.len(),
                    SidebarTab::History => tab.query_history.entries().len(),
                };
                let selected = tab.sidebar_selected;
                if selected < max.saturating_sub(1) {
                    self.tab_mut().sidebar_selected = selected + 1;
                }
            }
            KeyCode::Enter => {
                self.handle_sidebar_select().await?;
            }
            KeyCode::Char('1') => {
                let tab = self.tab_mut();
                tab.sidebar_tab = SidebarTab::Databases;
                tab.sidebar_selected = 0;
            }
            KeyCode::Char('2') => {
                let tab = self.tab_mut();
                tab.sidebar_tab = SidebarTab::Tables;
                tab.sidebar_selected = 0;
            }
            KeyCode::Char('3') => {
                let tab = self.tab_mut();
                tab.sidebar_tab = SidebarTab::History;
                tab.sidebar_selected = 0;
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.focus = Focus::ConnectionDialog;
                self.connection_dialog.active = true;
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_editor_input(&mut self, key: KeyEvent) -> Result<()> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);

        match key.code {
            KeyCode::Tab if !ctrl => {
                if shift {
                    self.focus = Focus::Sidebar;
                } else {
                    self.tab_mut().editor.insert_tab();
                }
            }
            KeyCode::BackTab => {
                self.focus = Focus::Sidebar;
            }
            KeyCode::Enter if ctrl => {
                // Execute query
                self.execute_query().await?;
                self.focus = Focus::Results;
            }
            KeyCode::F(5) => {
                // F5 also executes query (works in all terminals)
                self.execute_query().await?;
                self.focus = Focus::Results;
            }
            KeyCode::Enter => {
                self.tab_mut().editor.insert_newline();
            }
            KeyCode::Char('c') if ctrl => {
                self.tab_mut().editor.copy();
            }
            KeyCode::Char('x') if ctrl => {
                self.tab_mut().editor.cut();
            }
            KeyCode::Char('v') if ctrl => {
                self.tab_mut().editor.paste();
            }
            KeyCode::Char('a') if ctrl => {
                self.tab_mut().editor.select_all();
            }
            KeyCode::Char('z') if ctrl => {
                // Undo (not implemented yet)
            }
            KeyCode::Char('l') if ctrl => {
                // Clear editor
                self.tab_mut().editor.clear();
            }
            KeyCode::Up if ctrl => {
                // Previous in history
                let tab = self.tab_mut();
                if let Some(entry) = tab.query_history.previous() {
                    let query = entry.query.clone();
                    tab.editor.set_text(&query);
                }
            }
            KeyCode::Down if ctrl => {
                // Next in history
                let tab = self.tab_mut();
                if let Some(entry) = tab.query_history.next() {
                    let query = entry.query.clone();
                    tab.editor.set_text(&query);
                }
            }
            KeyCode::Char(c) => {
                self.tab_mut().editor.insert_char(c);
            }
            KeyCode::Backspace => {
                self.tab_mut().editor.backspace();
            }
            KeyCode::Delete => {
                self.tab_mut().editor.delete();
            }
            KeyCode::Left if ctrl => {
                self.tab_mut().editor.move_word_left();
            }
            KeyCode::Right if ctrl => {
                self.tab_mut().editor.move_word_right();
            }
            KeyCode::Left => {
                self.tab_mut().editor.move_left();
            }
            KeyCode::Right => {
                self.tab_mut().editor.move_right();
            }
            KeyCode::Up => {
                self.tab_mut().editor.move_up();
            }
            KeyCode::Down => {
                self.tab_mut().editor.move_down();
            }
            KeyCode::Home if ctrl => {
                self.tab_mut().editor.move_to_start();
            }
            KeyCode::End if ctrl => {
                self.tab_mut().editor.move_to_end();
            }
            KeyCode::Home => {
                self.tab_mut().editor.move_to_line_start();
            }
            KeyCode::End => {
                self.tab_mut().editor.move_to_line_end();
            }
            KeyCode::Esc => {
                if self.tab().editor.has_selection() {
                    self.tab_mut().editor.clear_selection();
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_results_input(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.focus = Focus::Editor;
            }
            KeyCode::BackTab => {
                self.focus = Focus::Editor;
            }
            KeyCode::Tab => {
                self.focus = Focus::Sidebar;
            }
            KeyCode::Up if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.focus = Focus::Editor;
            }
            KeyCode::Left => {
                let tab = self.tab_mut();
                if tab.result_selected_col > 0 {
                    tab.result_selected_col -= 1;
                }
            }
            KeyCode::Right => {
                let tab = self.tab_mut();
                if let Some(result) = tab.results.get(tab.current_result) {
                    let max = result.columns.len().saturating_sub(1);
                    if tab.result_selected_col < max {
                        tab.result_selected_col += 1;
                    }
                }
            }
            KeyCode::Up => {
                let tab = self.tab_mut();
                if tab.result_selected_row > 0 {
                    tab.result_selected_row -= 1;
                }
            }
            KeyCode::Down => {
                let tab = self.tab_mut();
                if let Some(result) = tab.results.get(tab.current_result) {
                    let max = result.rows.len().saturating_sub(1);
                    if tab.result_selected_row < max {
                        tab.result_selected_row += 1;
                    }
                }
            }
            KeyCode::Home => {
                self.tab_mut().result_selected_col = 0;
            }
            KeyCode::End => {
                let tab = self.tab_mut();
                if let Some(result) = tab.results.get(tab.current_result) {
                    tab.result_selected_col = result.columns.len().saturating_sub(1);
                }
            }
            KeyCode::PageUp => {
                let tab = self.tab_mut();
                tab.result_selected_row = tab.result_selected_row.saturating_sub(20);
            }
            KeyCode::PageDown => {
                let tab = self.tab_mut();
                if let Some(result) = tab.results.get(tab.current_result) {
                    let max = result.rows.len().saturating_sub(1);
                    tab.result_selected_row = (tab.result_selected_row + 20).min(max);
                }
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.copy_selected_cell();
            }
            KeyCode::Char('[') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let tab = self.tab_mut();
                if tab.current_result > 0 {
                    tab.current_result -= 1;
                    tab.result_selected_row = 0;
                    tab.result_selected_col = 0;
                }
            }
            KeyCode::Char(']') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let tab = self.tab_mut();
                if tab.current_result < tab.results.len().saturating_sub(1) {
                    tab.current_result += 1;
                    tab.result_selected_row = 0;
                    tab.result_selected_col = 0;
                }
            }
            _ => {}
        }
        Ok(())
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

    async fn handle_sidebar_select(&mut self) -> Result<()> {
        let sidebar_tab = self.tab().sidebar_tab;
        let sidebar_selected = self.tab().sidebar_selected;
        match sidebar_tab {
            SidebarTab::Databases => {
                if let Some(db) = self.tab().databases.get(sidebar_selected) {
                    let db_name = db.name.clone();
                    self.tab_mut().connection.switch_database(&db_name).await?;
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
                let tab = self.tab();
                let schemas: Vec<_> = tab.schemas.to_vec();
                let tables: Vec<_> = tab.tables.to_vec();
                let expanded: Vec<_> = tab.expanded_schemas.clone();

                for schema in &schemas {
                    if index == sidebar_selected {
                        // Toggle schema expansion
                        let tab = self.tab_mut();
                        if expanded.contains(&schema.name) {
                            tab.expanded_schemas.retain(|s| s != &schema.name);
                        } else {
                            tab.expanded_schemas.push(schema.name.clone());
                        }
                        return Ok(());
                    }
                    index += 1;

                    if expanded.contains(&schema.name) {
                        for table in &tables {
                            if table.schema == schema.name {
                                if index == sidebar_selected {
                                    // Insert table name into editor
                                    let full_name = format!("{}.{}", table.schema, table.name);
                                    self.tab_mut().editor.insert_text(&full_name);
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
                let entries = self.tab().query_history.entries();
                if let Some(entry) = entries.get(entries.len() - 1 - sidebar_selected) {
                    let query = entry.query.clone();
                    self.tab_mut().editor.set_text(&query);
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
        if self.tab().pending_connection.is_some() {
            return;
        }

        self.connection_dialog.status_message = Some((
            format!("Connecting to {}...", config.display_string()),
            StatusType::Info,
        ));
        self.start_loading(format!("Connecting to {}...", config.display_string()));

        let config_for_task = config.clone();
        let handle = tokio::spawn(async move { create_client(&config_for_task).await });
        self.tab_mut().pending_connection = Some((config, handle));
    }

    async fn finish_connect(&mut self, config: ConnectionConfig, client: Client) -> Result<()> {
        self.tab_mut()
            .connection
            .apply_client(config.clone(), client);
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
        if self.tab().connection.client.is_some() {
            self.start_loading("Loading schema...".to_string());

            let client = self.tab().connection.client.as_ref().unwrap();

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

            let tab = self.tab_mut();
            tab.databases = databases;
            tab.schemas = schemas;
            tab.tables = all_tables;
            self.stop_loading();
        }
        Ok(())
    }

    async fn execute_query(&mut self) -> Result<()> {
        let query = self.tab().editor.text();
        if query.trim().is_empty() {
            return Ok(());
        }

        if self.tab().connection.client.is_some() {
            self.start_loading("Executing query...".to_string());

            let client = self.tab().connection.client.as_ref().unwrap();
            let result = execute_query(client, &query).await?;
            self.stop_loading();

            // Add to history
            let db = self.tab().connection.current_database.clone();
            let entry = HistoryEntry {
                query: query.clone(),
                timestamp: chrono::Utc::now(),
                database: db,
                execution_time_ms: result.execution_time.as_millis() as u64,
                success: result.error.is_none(),
            };
            self.tab_mut().query_history.add(entry);
            let _ = self.tab_mut().query_history.save();

            // Update status
            if let Some(err) = &result.error {
                self.set_status(format!("Error: {}", err), StatusType::Error);
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

            let tab = self.tab_mut();
            tab.results.push(result);
            tab.current_result = tab.results.len() - 1;
            tab.result_selected_row = 0;
            tab.result_selected_col = 0;
        } else {
            self.set_status("Not connected to database".to_string(), StatusType::Error);
        }

        Ok(())
    }

    fn copy_selected_cell(&mut self) {
        let tab = self.tab();
        if let Some(result) = tab.results.get(tab.current_result) {
            if let Some(row) = result.rows.get(tab.result_selected_row) {
                if let Some(cell) = row.get(tab.result_selected_col) {
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

        // Poll pending connection task on active tab
        let has_pending = self
            .tab()
            .pending_connection
            .as_ref()
            .is_some_and(|(_, h)| h.is_finished());

        if has_pending {
            let (config, handle) = self.tab_mut().pending_connection.take().unwrap();
            match handle.await {
                Ok(Ok(client)) => {
                    self.finish_connect(config, client).await?;
                }
                Ok(Err(e)) => {
                    self.stop_loading();
                    let msg = format!("Connection failed: {}", e);
                    self.connection_dialog.status_message = Some((msg.clone(), StatusType::Error));
                    self.set_status(msg, StatusType::Error);
                }
                Err(e) => {
                    self.stop_loading();
                    let msg = format!("Connection task failed: {}", e);
                    self.connection_dialog.status_message = Some((msg.clone(), StatusType::Error));
                    self.set_status(msg, StatusType::Error);
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
