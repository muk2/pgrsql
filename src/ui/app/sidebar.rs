use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::db::{get_columns, get_databases, get_indexes, get_schemas, get_table_ddl, get_tables};

use super::{App, Focus, SidebarTab, StatusType, TableInspectorState};

impl App {
    pub(super) async fn handle_sidebar_input(&mut self, key: KeyEvent) -> Result<()> {
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

    pub(super) async fn refresh_schema(&mut self) -> Result<()> {
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
}
