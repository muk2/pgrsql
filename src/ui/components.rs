use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Table, Tabs, Wrap},
    Frame,
};

use crate::db::SslMode;
use crate::ui::{
    is_sql_keyword, is_sql_type, App, Focus, SidebarTab, StatusType, Theme, SPINNER_FRAMES,
};

pub fn draw(frame: &mut Frame, app: &App) {
    // Create main layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Header
            Constraint::Min(0),    // Main content
            Constraint::Length(1), // Status bar
        ])
        .split(frame.area());

    // Draw header
    draw_header(frame, app, chunks[0]);

    // Draw main content
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(app.sidebar_width), Constraint::Min(0)])
        .split(chunks[1]);

    draw_sidebar(frame, app, main_chunks[0]);
    draw_main_panel(frame, app, main_chunks[1]);

    // Draw status bar
    draw_status_bar(frame, app, chunks[2]);

    // Draw toasts
    if !app.show_help {
        draw_toasts(frame, app);
    }

    // Draw connection dialog if active
    if app.connection_dialog.active {
        draw_connection_dialog(frame, app);
    }

    // Draw help overlay if active
    if app.show_help {
        draw_help_overlay(frame, app);
    }
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;

    let connection_info = if app.connection.is_connected() {
        format!(
            " {} | {} | {} ",
            app.connection.config.display_string(),
            app.connection.current_database,
            app.connection.current_schema
        )
    } else {
        " Not Connected ".to_string()
    };

    let header_text = format!(
        " pgrsql {}{}",
        connection_info,
        " ".repeat(area.width.saturating_sub(connection_info.len() as u16 + 10) as usize)
    );

    let header = Paragraph::new(header_text).style(theme.header());

    frame.render_widget(header, area);
}

fn draw_sidebar(frame: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;
    let focused = app.focus == Focus::Sidebar;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Tabs
            Constraint::Min(0),    // Content
        ])
        .split(area);

    // Draw tabs
    let tab_titles = vec!["Databases", "Tables", "History"];
    let selected_tab = match app.sidebar_tab {
        SidebarTab::Databases => 0,
        SidebarTab::Tables => 1,
        SidebarTab::History => 2,
    };

    let tabs = Tabs::new(tab_titles)
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(theme.border_style(focused)),
        )
        .select(selected_tab)
        .style(Style::default().fg(theme.text_secondary))
        .highlight_style(
            Style::default()
                .fg(theme.text_accent)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(tabs, chunks[0]);

    // Draw content based on selected tab
    match app.sidebar_tab {
        SidebarTab::Databases => draw_databases_list(frame, app, chunks[1]),
        SidebarTab::Tables => draw_tables_tree(frame, app, chunks[1]),
        SidebarTab::History => draw_history_list(frame, app, chunks[1]),
    }
}

fn draw_databases_list(frame: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;
    let focused = app.focus == Focus::Sidebar;

    let items: Vec<ListItem> = app
        .databases
        .iter()
        .enumerate()
        .map(|(i, db)| {
            let style = if i == app.sidebar_selected {
                theme.selected()
            } else if db.name == app.connection.current_database {
                Style::default()
                    .fg(theme.text_accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_primary)
            };

            ListItem::new(format!("  {}", db.name)).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border_style(focused))
            .title(" Databases ")
            .title_style(if focused {
                Style::default().fg(theme.text_accent)
            } else {
                Style::default().fg(theme.text_secondary)
            }),
    );

    frame.render_widget(list, area);
}

fn draw_tables_tree(frame: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;
    let focused = app.focus == Focus::Sidebar;

    let mut items: Vec<ListItem> = Vec::new();
    let mut index = 0;

    for schema in &app.schemas {
        let expanded = app.expanded_schemas.contains(&schema.name);
        let icon = if expanded { "▼" } else { "▶" };

        let style = if index == app.sidebar_selected {
            theme.selected()
        } else {
            Style::default().fg(theme.text_accent)
        };

        items.push(ListItem::new(format!(" {} {}", icon, schema.name)).style(style));
        index += 1;

        if expanded {
            for table in &app.tables {
                if table.schema == schema.name {
                    let table_icon = match table.table_type {
                        crate::db::TableType::Table => "󰓫",
                        crate::db::TableType::View => "󰈈",
                        crate::db::TableType::MaterializedView => "󰈈",
                        crate::db::TableType::ForeignTable => "󰒍",
                    };

                    let style = if index == app.sidebar_selected {
                        theme.selected()
                    } else {
                        Style::default().fg(theme.text_primary)
                    };

                    items.push(
                        ListItem::new(format!("   {} {}", table_icon, table.name)).style(style),
                    );
                    index += 1;
                }
            }
        }
    }

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border_style(focused))
            .title(" Tables ")
            .title_style(if focused {
                Style::default().fg(theme.text_accent)
            } else {
                Style::default().fg(theme.text_secondary)
            }),
    );

    frame.render_widget(list, area);
}

fn draw_history_list(frame: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;
    let focused = app.focus == Focus::Sidebar;

    let entries = app.query_history.entries();
    let items: Vec<ListItem> = entries
        .iter()
        .rev()
        .enumerate()
        .map(|(i, entry)| {
            let status_icon = if entry.success { "✓" } else { "✗" };
            let query_preview: String = entry
                .query
                .chars()
                .take(30)
                .collect::<String>()
                .replace('\n', " ");

            let style = if i == app.sidebar_selected {
                theme.selected()
            } else {
                Style::default().fg(theme.text_primary)
            };

            ListItem::new(format!(" {} {}", status_icon, query_preview)).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border_style(focused))
            .title(" History ")
            .title_style(if focused {
                Style::default().fg(theme.text_accent)
            } else {
                Style::default().fg(theme.text_secondary)
            }),
    );

    frame.render_widget(list, area);
}

fn draw_main_panel(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(app.editor_height_percent), // Editor (resizable)
            Constraint::Min(0),                                // Results
        ])
        .split(area);

    draw_editor(frame, app, chunks[0]);
    draw_results(frame, app, chunks[1]);
}

fn draw_editor(frame: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;
    let focused = app.focus == Focus::Editor;

    let inner_area = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border_style(focused))
        .title(" Query Editor (F5 or Ctrl+Enter to execute) ")
        .title_style(if focused {
            Style::default().fg(theme.text_accent)
        } else {
            Style::default().fg(theme.text_secondary)
        })
        .inner(area);

    // Render block
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border_style(focused))
            .title(" Query Editor (F5 or Ctrl+Enter to execute) ")
            .title_style(if focused {
                Style::default().fg(theme.text_accent)
            } else {
                Style::default().fg(theme.text_secondary)
            }),
        area,
    );

    // Determine active query range for visual highlighting
    let query_range = app.get_current_query_line_range();

    // Syntax highlight and render editor content
    let visible_height = inner_area.height as usize;
    let lines: Vec<Line> = app
        .editor
        .lines
        .iter()
        .skip(app.editor.scroll_offset)
        .take(visible_height)
        .enumerate()
        .map(|(line_idx, line_text)| {
            let actual_line = line_idx + app.editor.scroll_offset;
            let in_active_query = query_range
                .map(|(start, end)| actual_line >= start && actual_line <= end)
                .unwrap_or(false);
            highlight_sql_line(line_text, theme, actual_line, &app.editor, in_active_query)
        })
        .collect();

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner_area);

    // Show cursor (offset by 2 for gutter prefix)
    if focused {
        let cursor_x = inner_area.x + 2 + app.editor.cursor_x as u16;
        let cursor_y = inner_area.y + (app.editor.cursor_y - app.editor.scroll_offset) as u16;
        if cursor_y < inner_area.y + inner_area.height {
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }
}

fn highlight_sql_line<'a>(
    line: &'a str,
    theme: &Theme,
    line_number: usize,
    editor: &crate::editor::TextBuffer,
    in_active_query: bool,
) -> Line<'a> {
    let mut spans: Vec<Span> = Vec::new();
    let mut current_word = String::new();
    let mut in_string = false;
    let mut string_char = '"';
    let in_comment = false;

    // Show a gutter marker for the active query block
    if in_active_query {
        spans.push(Span::styled(
            "\u{2502} ".to_string(), // "│ " vertical bar
            Style::default().fg(theme.text_accent),
        ));
    } else {
        spans.push(Span::styled(
            "  ".to_string(),
            Style::default().fg(theme.text_muted),
        ));
    }

    for (i, c) in line.char_indices() {
        // Check for selection
        let is_selected = if let Some(((start_x, start_y), (end_x, end_y))) = editor.get_selection()
        {
            if start_y == end_y && line_number == start_y {
                i >= start_x && i < end_x
            } else if line_number == start_y {
                i >= start_x
            } else if line_number == end_y {
                i < end_x
            } else {
                line_number > start_y && line_number < end_y
            }
        } else {
            false
        };

        let base_style = if is_selected {
            Style::default().bg(theme.selection)
        } else {
            Style::default()
        };

        // Handle comments
        if !in_string && line[i..].starts_with("--") {
            if !current_word.is_empty() {
                spans.push(create_word_span(&current_word, theme, base_style));
                current_word.clear();
            }
            spans.push(Span::styled(
                line[i..].to_string(),
                base_style.fg(theme.syntax_comment),
            ));
            break;
        }

        // Handle strings
        if (c == '\'' || c == '"') && !in_comment {
            if in_string && c == string_char {
                current_word.push(c);
                spans.push(Span::styled(
                    current_word.clone(),
                    base_style.fg(theme.syntax_string),
                ));
                current_word.clear();
                in_string = false;
            } else if !in_string {
                if !current_word.is_empty() {
                    spans.push(create_word_span(&current_word, theme, base_style));
                    current_word.clear();
                }
                in_string = true;
                string_char = c;
                current_word.push(c);
            } else {
                current_word.push(c);
            }
            continue;
        }

        if in_string {
            current_word.push(c);
            continue;
        }

        // Handle word boundaries
        if c.is_alphanumeric() || c == '_' {
            current_word.push(c);
        } else {
            if !current_word.is_empty() {
                spans.push(create_word_span(&current_word, theme, base_style));
                current_word.clear();
            }

            // Handle operators and punctuation
            let style = match c {
                '(' | ')' | '[' | ']' | '{' | '}' => base_style.fg(theme.text_primary),
                ',' | ';' => base_style.fg(theme.text_secondary),
                '=' | '>' | '<' | '!' | '+' | '-' | '*' | '/' | '%' => {
                    base_style.fg(theme.syntax_operator)
                }
                _ => base_style.fg(theme.text_primary),
            };
            spans.push(Span::styled(c.to_string(), style));
        }
    }

    // Handle remaining word
    if !current_word.is_empty() {
        let style = if in_string {
            Style::default().fg(theme.syntax_string)
        } else {
            Style::default()
        };
        spans.push(create_word_span(&current_word, theme, style));
    }

    Line::from(spans)
}

fn create_word_span<'a>(word: &str, theme: &Theme, base_style: Style) -> Span<'a> {
    let style = if is_sql_keyword(word) {
        base_style
            .fg(theme.syntax_keyword)
            .add_modifier(Modifier::BOLD)
    } else if is_sql_type(word) {
        base_style.fg(theme.syntax_type)
    } else if word.chars().all(|c| c.is_ascii_digit() || c == '.') {
        base_style.fg(theme.syntax_number)
    } else {
        base_style.fg(theme.text_primary)
    };
    Span::styled(word.to_string(), style)
}

fn draw_results(frame: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;
    let focused = app.focus == Focus::Results;

    let result_index = if app.results.is_empty() {
        0
    } else {
        app.current_result + 1
    };
    let result_total = app.results.len();

    // Build title with execution time, row count, and cell position
    let title = if let Some(result) = app.results.get(app.current_result) {
        let time_ms = result.execution_time.as_secs_f64() * 1000.0;
        let position = if !result.columns.is_empty() && !result.rows.is_empty() {
            format!(
                " [R{}/C{}]",
                app.result_selected_row + 1,
                app.result_selected_col + 1
            )
        } else {
            String::new()
        };
        if result.error.is_some() {
            format!(
                " Results ({}/{}) - ERROR ({:.2}ms) ",
                result_index, result_total, time_ms
            )
        } else if let Some(affected) = result.affected_rows {
            format!(
                " Results ({}/{}) - {} rows affected ({:.2}ms) ",
                result_index, result_total, affected, time_ms
            )
        } else {
            format!(
                " Results ({}/{}) - {} rows x {} cols ({:.2}ms){} ",
                result_index,
                result_total,
                result.row_count,
                result.columns.len(),
                time_ms,
                position
            )
        }
    } else {
        format!(" Results ({}/{}) ", result_index, result_total)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border_style(focused))
        .title(title)
        .title_style(if focused {
            Style::default().fg(theme.text_accent)
        } else {
            Style::default().fg(theme.text_secondary)
        });

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let Some(result) = app.results.get(app.current_result) {
        if let Some(error) = &result.error {
            let error_text = Paragraph::new(error.as_str())
                .style(theme.status_error())
                .wrap(Wrap { trim: true });
            frame.render_widget(error_text, inner);
        } else if result.columns.is_empty() {
            if let Some(affected) = result.affected_rows {
                let msg = format!("{} rows affected", affected);
                let text = Paragraph::new(msg).style(theme.status_success());
                frame.render_widget(text, inner);
            }
        } else {
            draw_result_table(frame, app, result, inner);
        }
    } else {
        let text = Paragraph::new("No results yet. Execute a query with F5 or Ctrl+Enter.")
            .style(theme.muted());
        frame.render_widget(text, inner);
    }
}

fn draw_result_table(frame: &mut Frame, app: &App, result: &crate::db::QueryResult, area: Rect) {
    let theme = &app.theme;

    // Calculate column widths
    let col_widths: Vec<Constraint> = result
        .columns
        .iter()
        .map(|col| {
            let width = col.max_width.min(40).max(col.name.len()) + 2;
            Constraint::Length(width as u16)
        })
        .collect();

    // Create header
    let header_cells: Vec<Cell> = result
        .columns
        .iter()
        .enumerate()
        .map(|(i, col)| {
            let style = if i == app.result_selected_col {
                Style::default()
                    .fg(theme.text_accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD)
            };
            Cell::from(col.name.clone()).style(style)
        })
        .collect();

    let header = Row::new(header_cells)
        .style(Style::default().bg(theme.bg_secondary))
        .height(1);

    // Create rows
    let visible_height = area.height.saturating_sub(2) as usize;
    let start_row = app.result_scroll_y;

    let rows: Vec<Row> = result
        .rows
        .iter()
        .enumerate()
        .skip(start_row)
        .take(visible_height)
        .map(|(row_idx, row)| {
            let cells: Vec<Cell> = row
                .iter()
                .enumerate()
                .map(|(col_idx, cell)| {
                    let display = cell.display();
                    let truncated: String = display.chars().take(40).collect();

                    let style = if row_idx == app.result_selected_row {
                        if col_idx == app.result_selected_col {
                            Style::default()
                                .bg(theme.bg_highlight)
                                .fg(theme.text_accent)
                        } else {
                            Style::default().bg(theme.bg_selected)
                        }
                    } else if matches!(cell, crate::db::CellValue::Null) {
                        Style::default().fg(theme.text_muted)
                    } else {
                        Style::default().fg(theme.text_primary)
                    };

                    Cell::from(truncated).style(style)
                })
                .collect();

            Row::new(cells).height(1)
        })
        .collect();

    let table = Table::new(rows, &col_widths)
        .header(header)
        .highlight_style(theme.selected());

    frame.render_widget(table, area);
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;

    // Left section: spinner + loading message OR connection status
    let left_text = if app.is_loading {
        let spinner = SPINNER_FRAMES[app.spinner_frame];
        format!(" {} {}", spinner, app.loading_message)
    } else if app.connection.is_connected() {
        format!(" Connected: {}", app.connection.config.display_string())
    } else {
        " Disconnected".to_string()
    };

    let left_style = if app.is_loading {
        Style::default().fg(theme.info).bg(theme.bg_secondary)
    } else if app.connection.is_connected() {
        Style::default().fg(theme.success).bg(theme.bg_secondary)
    } else {
        Style::default().fg(theme.text_muted).bg(theme.bg_secondary)
    };

    // Right section: help hints
    let right_text = "? Help | Ctrl+Q/D Quit ";

    // Calculate padding
    let left_len = left_text.len() as u16;
    let right_len = right_text.len() as u16;
    let padding = area.width.saturating_sub(left_len + right_len);

    let status_line = Line::from(vec![
        Span::styled(left_text, left_style),
        Span::styled(
            " ".repeat(padding as usize),
            Style::default().bg(theme.bg_secondary),
        ),
        Span::styled(
            right_text.to_string(),
            Style::default().fg(theme.text_muted).bg(theme.bg_secondary),
        ),
    ]);

    let status = Paragraph::new(status_line);
    frame.render_widget(status, area);
}

fn draw_connection_dialog(frame: &mut Frame, app: &App) {
    let theme = &app.theme;
    let dialog = &app.connection_dialog;

    // Calculate dialog size and position (taller to fit saved connections list)
    let area = frame.area();
    let dialog_width = 80.min(area.width.saturating_sub(4));
    let dialog_height = 25.min(area.height.saturating_sub(4));

    let dialog_x = (area.width - dialog_width) / 2;
    let dialog_y = (area.height - dialog_height) / 2;

    let dialog_area = Rect::new(dialog_x, dialog_y, dialog_width, dialog_height);

    // Clear the background
    frame.render_widget(Clear, dialog_area);

    // Draw dialog block
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_focused))
        .title(" Connect to PostgreSQL ")
        .title_style(
            Style::default()
                .fg(theme.text_accent)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().bg(theme.bg_primary));

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    // Draw form fields
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Name
            Constraint::Length(2), // Host
            Constraint::Length(2), // Port
            Constraint::Length(2), // Database
            Constraint::Length(2), // Username
            Constraint::Length(2), // Password
            Constraint::Length(2), // SSL Mode
            Constraint::Length(1), // Status message
            Constraint::Length(1), // Buttons
            Constraint::Min(0),    // Saved connections
        ])
        .split(inner);

    let label_width: u16 = 14; // " {:12} " = 1 + 12 + 1 chars
    let available_width = inner.width.saturating_sub(label_width + 1) as usize;

    let field_labels = [
        "Name:",
        "Host:",
        "Port:",
        "Database:", // optional — defaults to "postgres"
        "Username:",
        "Password:",
    ];
    let field_placeholders: [&str; 6] = ["", "", "", "postgres", "", ""];
    let port_string = dialog.config.port.to_string();
    let password_display = "*".repeat(dialog.config.password.len());
    let field_values: [&str; 6] = [
        &dialog.config.name,
        &dialog.config.host,
        &port_string,
        &dialog.config.database,
        &dialog.config.username,
        &password_display,
    ];

    for (i, (label, value)) in field_labels.iter().zip(field_values.iter()).enumerate() {
        let is_focused = dialog.field_index == i;

        let style = if is_focused {
            Style::default().fg(theme.text_accent)
        } else {
            Style::default().fg(theme.text_primary)
        };

        // Horizontal scroll: keep cursor visible within the available width
        let cursor_pos = dialog.field_cursors[i];
        let (display_value, cursor_display_x) = if value.len() > available_width {
            let scroll = if cursor_pos > available_width.saturating_sub(2) {
                cursor_pos.saturating_sub(available_width.saturating_sub(2))
            } else {
                0
            };
            let end = (scroll + available_width).min(value.len());
            (&value[scroll..end], cursor_pos - scroll)
        } else {
            (*value, cursor_pos)
        };

        let (text, final_style) = if display_value.is_empty() && !field_placeholders[i].is_empty() {
            (
                format!(" {:12} {}", label, field_placeholders[i]),
                Style::default().fg(theme.text_muted),
            )
        } else {
            (format!(" {:12} {}", label, display_value), style)
        };
        let paragraph = Paragraph::new(text).style(final_style);
        frame.render_widget(paragraph, chunks[i]);

        // Set terminal cursor for the focused field
        if is_focused {
            let cursor_x = chunks[i].x + label_width + cursor_display_x as u16;
            let cursor_y = chunks[i].y;
            if cursor_x < chunks[i].x + chunks[i].width {
                frame.set_cursor_position((cursor_x, cursor_y));
            }
        }
    }

    // SSL Mode field (field index 6)
    let ssl_focused = dialog.field_index == 6;
    let ssl_style = if ssl_focused {
        Style::default().fg(theme.text_accent)
    } else {
        Style::default().fg(theme.text_primary)
    };
    let ssl_value = match dialog.config.ssl_mode {
        SslMode::Disable => "Disable",
        SslMode::Prefer => "Prefer",
        SslMode::Require => "Require (no verify)",
        SslMode::VerifyCa => "Verify-CA",
        SslMode::VerifyFull => "Verify-Full",
    };
    let ssl_hint = if ssl_focused {
        " (←/→ to change)"
    } else {
        ""
    };
    let ssl_text = format!(" {:12} {}{}", "SSL Mode:", ssl_value, ssl_hint);
    let ssl_paragraph = Paragraph::new(ssl_text).style(ssl_style);
    frame.render_widget(ssl_paragraph, chunks[6]);

    // Draw inline status message
    if let Some((ref msg, ref status_type)) = dialog.status_message {
        let color = match status_type {
            StatusType::Info => theme.text_accent,
            StatusType::Success => theme.success,
            StatusType::Warning => theme.warning,
            StatusType::Error => theme.error,
        };
        let status_line = if matches!(status_type, StatusType::Info) && app.is_loading {
            let spinner = SPINNER_FRAMES[app.spinner_frame % SPINNER_FRAMES.len()];
            format!(" {} {}", spinner, msg)
        } else {
            format!(" {}", msg)
        };
        let status = Paragraph::new(status_line).style(Style::default().fg(color));
        frame.render_widget(status, chunks[7]);
    }

    // Draw dynamic hint text
    let button_text = if app.pending_connection.is_some() {
        " Esc to cancel "
    } else if dialog.selected_saved.is_some() {
        " Enter to load | Del to delete | Tab to switch fields | Esc to cancel "
    } else {
        " Enter to connect | Tab to switch fields | Esc to cancel "
    };
    let button = Paragraph::new(button_text).style(Style::default().fg(theme.text_muted));
    frame.render_widget(button, chunks[8]);

    // Draw saved connections list
    if !dialog.saved_connections.is_empty() {
        let saved_area = chunks[9];

        // Title line
        let title = Paragraph::new(" Saved connections (↑/↓ to select):")
            .style(Style::default().fg(theme.text_secondary));

        // We need to split the saved_area into title + list
        if saved_area.height > 1 {
            let saved_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1), // Title
                    Constraint::Min(0),    // List items
                ])
                .split(saved_area);

            frame.render_widget(title, saved_chunks[0]);

            let items: Vec<ListItem> = dialog
                .saved_connections
                .iter()
                .enumerate()
                .map(|(i, conn)| {
                    let is_selected = dialog.selected_saved == Some(i);
                    let prefix = if is_selected { " > " } else { "   " };
                    let display = format!("{}{} ({})", prefix, conn.name, conn.display_string());
                    let style = if is_selected {
                        Style::default()
                            .fg(theme.text_accent)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.text_primary)
                    };
                    ListItem::new(display).style(style)
                })
                .collect();

            let list = List::new(items);
            frame.render_widget(list, saved_chunks[1]);
        } else {
            frame.render_widget(title, saved_area);
        }
    }
}

fn draw_toasts(frame: &mut Frame, app: &App) {
    let theme = &app.theme;
    if app.toasts.is_empty() {
        return;
    }

    let area = frame.area();
    let toast_width = 50.min(area.width.saturating_sub(4));

    // Stack toasts upward from the bottom-right, above the status bar (1 line)
    for (i, toast) in app.toasts.iter().rev().enumerate() {
        let toast_y = area.height.saturating_sub(2 + i as u16); // 1 for status bar, 1 per toast
        if toast_y < 1 {
            break; // Don't draw above the header
        }

        let toast_x = area.width.saturating_sub(toast_width + 1);
        let toast_area = Rect::new(toast_x, toast_y, toast_width, 1);

        let icon = match toast.status_type {
            StatusType::Success => "✓",
            StatusType::Error => "✗",
            StatusType::Warning => "!",
            StatusType::Info => "ℹ",
        };

        let (fg, bg) = match toast.status_type {
            StatusType::Success => (theme.bg_primary, theme.success),
            StatusType::Error => (theme.bg_primary, theme.error),
            StatusType::Warning => (theme.bg_primary, theme.warning),
            StatusType::Info => (theme.bg_primary, theme.info),
        };

        // Fade effect: dim text when toast progress > 80%
        let style = if toast.progress() > 0.8 {
            Style::default().fg(fg).bg(bg).add_modifier(Modifier::DIM)
        } else {
            Style::default().fg(fg).bg(bg)
        };

        // Truncate message to fit
        let max_msg_len = (toast_width as usize).saturating_sub(4); // icon + spaces + padding
        let msg: String = toast.message.chars().take(max_msg_len).collect();
        let text = format!(" {} {} ", icon, msg);

        frame.render_widget(Clear, toast_area);
        let paragraph = Paragraph::new(text).style(style);
        frame.render_widget(paragraph, toast_area);
    }
}

fn draw_help_overlay(frame: &mut Frame, app: &App) {
    let theme = &app.theme;
    let area = frame.area();

    let help_width = 50.min(area.width - 4);
    let help_height = 25.min(area.height - 4);

    let help_x = (area.width - help_width) / 2;
    let help_y = (area.height - help_height) / 2;

    let help_area = Rect::new(help_x, help_y, help_width, help_height);

    frame.render_widget(Clear, help_area);

    let help_text = vec![
        "",
        " KEYBOARD SHORTCUTS",
        " ══════════════════════════════════════",
        "",
        " GLOBAL",
        "   Ctrl+Q/D       Quit",
        "   Ctrl+C         Connect dialog",
        "   ?              Toggle help",
        "",
        " NAVIGATION",
        "   Tab             Next pane",
        "   Shift+Tab       Previous pane",
        "   (Sidebar → Editor → Results → ...)",
        "",
        " EDITOR",
        "   F5/Ctrl+Enter  Execute query at cursor",
        "   Ctrl+L         Clear editor",
        "   Ctrl+↑/↓       Navigate history",
        "   Ctrl+Shift+↑/↓ Resize editor/results",
        "   Ctrl+C/X/V     Copy/Cut/Paste",
        "   Ctrl+A         Select all",
        "   Tab            Insert spaces",
        "",
        " SIDEBAR",
        "   1/2/3          Switch tabs",
        "   Enter          Select item",
        "   ↑/↓            Navigate",
        "",
        " RESULTS",
        "   Tab/Shift+Tab  Next/Prev column",
        "   Arrow keys     Navigate cells",
        "   Esc            Back to editor",
        "   Ctrl+C         Copy cell value",
        "   Ctrl+[/]       Prev/Next result set",
        "   PageUp/Down    Scroll results",
        "",
    ];

    let text: Vec<Line> = help_text
        .iter()
        .map(|s| Line::from(Span::styled(*s, Style::default().fg(theme.text_primary))))
        .collect();

    let help = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border_focused))
                .title(" Help ")
                .title_style(
                    Style::default()
                        .fg(theme.text_accent)
                        .add_modifier(Modifier::BOLD),
                ),
        )
        .style(Style::default().bg(theme.bg_primary));

    frame.render_widget(help, help_area);
}
