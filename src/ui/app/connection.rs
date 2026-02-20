use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use tokio_postgres::Client;

use crate::db::{create_client, ConnectionConfig, ConnectionManager, SslMode};

use super::{App, Focus, StatusType};

impl App {
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

    pub(super) async fn handle_connection_dialog_input(&mut self, key: KeyEvent) -> Result<()> {
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

    pub(super) fn start_connect(&mut self) {
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

    pub(super) async fn finish_connect(
        &mut self,
        config: ConnectionConfig,
        client: Client,
    ) -> Result<()> {
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
