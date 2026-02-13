use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::{Duration, Instant};

use crate::db::{
    execute_query, get_databases, get_schemas, get_tables, ColumnDetails, ConnectionConfig,
    ConnectionManager, DatabaseInfo, QueryResult, SchemaInfo, SslMode, TableInfo,
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
pub enum TreeNode {
    Database(DatabaseInfo),
    Schema(SchemaInfo),
    Table(TableInfo),
    Column(ColumnDetails),
}

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
}

#[derive(Debug, Clone, Copy)]
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
        }
    }
}

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
        }
    }

    pub async fn try_auto_connect(&mut self, config: ConnectionConfig) {
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

        // Attempt connection
        match self.connect().await {
            Ok(()) if self.connection.is_connected() => {
                // Success — connect() already set focus to Editor and dismissed dialog
            }
            _ => {
                // Failure — connect() already showed error toast.
                // Ensure dialog stays visible with the config pre-filled so user can fix it.
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
        }
    }

    async fn handle_connection_dialog_input(&mut self, key: KeyEvent) -> Result<()> {
        let dialog = &mut self.connection_dialog;

        match key.code {
            KeyCode::Esc => {
                if self.connection.is_connected() {
                    dialog.active = false;
                    self.focus = Focus::Editor;
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
                        SslMode::Disable => SslMode::Require,
                        SslMode::Prefer => SslMode::Disable,
                        SslMode::Require => SslMode::Prefer,
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
                        SslMode::Require => SslMode::Disable,
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
                    self.connect().await?;
                }
            }
            KeyCode::Char(c) => {
                if dialog.field_index == 6 {
                    return Ok(());
                }
                dialog.selected_saved = None;
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
                    self.editor.insert_tab();
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
                self.editor.insert_newline();
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
            KeyCode::Char('z') if ctrl => {
                // Undo (not implemented yet)
            }
            KeyCode::Char('l') if ctrl => {
                // Clear editor
                self.editor.clear();
            }
            KeyCode::Up if ctrl => {
                // Previous in history
                if let Some(entry) = self.query_history.previous() {
                    self.editor.set_text(&entry.query);
                }
            }
            KeyCode::Down if ctrl => {
                // Next in history
                if let Some(entry) = self.query_history.next() {
                    self.editor.set_text(&entry.query);
                }
            }
            KeyCode::Char(c) => {
                self.editor.insert_char(c);
            }
            KeyCode::Backspace => {
                self.editor.backspace();
            }
            KeyCode::Delete => {
                self.editor.delete();
            }
            KeyCode::Left if ctrl => {
                self.editor.move_word_left();
            }
            KeyCode::Right if ctrl => {
                self.editor.move_word_right();
            }
            KeyCode::Left => {
                self.editor.move_left();
            }
            KeyCode::Right => {
                self.editor.move_right();
            }
            KeyCode::Up => {
                self.editor.move_up();
            }
            KeyCode::Down => {
                self.editor.move_down();
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
                }
            }
            KeyCode::Down => {
                if let Some(result) = self.results.get(self.current_result) {
                    if self.result_selected_row < result.rows.len().saturating_sub(1) {
                        self.result_selected_row += 1;
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
            }
            KeyCode::PageDown => {
                if let Some(result) = self.results.get(self.current_result) {
                    self.result_selected_row =
                        (self.result_selected_row + 20).min(result.rows.len().saturating_sub(1));
                }
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.copy_selected_cell();
            }
            KeyCode::Char('[') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.current_result > 0 {
                    self.current_result -= 1;
                    self.result_selected_row = 0;
                    self.result_selected_col = 0;
                }
            }
            KeyCode::Char(']') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.current_result < self.results.len().saturating_sub(1) {
                    self.current_result += 1;
                    self.result_selected_row = 0;
                    self.result_selected_col = 0;
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

    async fn connect(&mut self) -> Result<()> {
        let config = self.connection_dialog.config.clone();

        self.start_loading(format!("Connecting to {}...", config.display_string()));

        match self.connection.connect(config.clone()).await {
            Ok(()) => {
                self.stop_loading();
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
                    let _ = ConnectionManager::save_connections(
                        &self.connection_dialog.saved_connections,
                    );
                }

                // Save as last used connection
                let _ = ConnectionManager::save_last_connection(&config.name);

                self.refresh_schema().await?;
                self.set_status(
                    format!("Connected to {}", config.display_string()),
                    StatusType::Success,
                );
            }
            Err(e) => {
                self.stop_loading();
                self.set_status(format!("Connection failed: {}", e), StatusType::Error);
            }
        }
        Ok(())
    }

    async fn refresh_schema(&mut self) -> Result<()> {
        if self.connection.client.is_some() {
            self.start_loading("Loading schema...".to_string());

            let client = self.connection.client.as_ref().unwrap();
            let databases = get_databases(&client).await.unwrap_or_default();
            let schemas = get_schemas(&client).await.unwrap_or_default();

            // Get tables for all schemas
            let mut all_tables = Vec::new();
            for schema in &schemas {
                if let Ok(tables) = get_tables(&client, &schema.name).await {
                    all_tables.extend(tables);
                }
            }

            self.databases = databases;
            self.schemas = schemas;
            self.tables = all_tables;
            self.stop_loading();
        }
        Ok(())
    }

    async fn execute_query(&mut self) -> Result<()> {
        let query = self.editor.text();
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

            self.results.push(result);
            self.current_result = self.results.len() - 1;
            self.result_selected_row = 0;
            self.result_selected_col = 0;
        } else {
            self.set_status("Not connected to database".to_string(), StatusType::Error);
        }

        Ok(())
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
