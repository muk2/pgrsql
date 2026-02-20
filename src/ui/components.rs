use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Table, Tabs, Wrap},
    Frame,
};

use crate::db::SslMode;
use crate::explain::{
    format_duration_ms, node_color_class, rows_mismatch, NodeColorClass, PlanNode, QueryPlan,
};
use crate::ui::{
    is_sql_function, is_sql_keyword, is_sql_type, App, FindReplaceField, Focus, SidebarTab,
    StatusType, Theme, EXPORT_FORMATS, SPINNER_FRAMES,
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

    // Draw autocomplete popup (positioned relative to editor cursor)
    if app.autocomplete.active && app.focus == Focus::Editor {
        // Compute the editor inner area to position the popup
        let editor_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(40), Constraint::Min(0)])
            .split(main_chunks[1]);
        let editor_inner = Block::default()
            .borders(Borders::ALL)
            .inner(editor_chunks[0]);
        draw_autocomplete(frame, app, editor_inner);
    }

    // Draw toasts
    if !app.show_help {
        draw_toasts(frame, app);
    }

    // Draw table inspector if active
    if app.table_inspector.is_some() {
        draw_table_inspector(frame, app);
    }

    // Draw connection dialog if active
    if app.connection_dialog.active {
        draw_connection_dialog(frame, app);
    }

    // Draw export picker if active
    if app.focus == Focus::ExportPicker {
        draw_export_picker(frame, app);
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

    // Calculate find/replace bar height
    let find_bar_height = if app.find_replace.active {
        if app.find_replace.show_replace {
            2u16
        } else {
            1u16
        }
    } else {
        0u16
    };

    // Split inner area for find bar and editor content
    let (find_area, editor_content_area) =
        if find_bar_height > 0 && inner_area.height > find_bar_height {
            (
                Rect::new(
                    inner_area.x,
                    inner_area.y,
                    inner_area.width,
                    find_bar_height,
                ),
                Rect::new(
                    inner_area.x,
                    inner_area.y + find_bar_height,
                    inner_area.width,
                    inner_area.height - find_bar_height,
                ),
            )
        } else {
            (
                Rect::new(inner_area.x, inner_area.y, inner_area.width, 0),
                inner_area,
            )
        };

    // Draw find/replace bar
    if app.find_replace.active && find_area.height > 0 {
        draw_find_bar(frame, app, find_area);
    }

    // Determine active query range for visual highlighting
    let query_range = app.get_current_query_line_range();

    // Syntax highlight and render editor content
    let visible_height = editor_content_area.height as usize;
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
            highlight_sql_line(
                line_text,
                theme,
                actual_line,
                &app.editor,
                in_active_query,
                &app.find_replace,
            )
        })
        .collect();

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, editor_content_area);

    // Show cursor
    if focused && !app.find_replace.active {
        let cursor_x = editor_content_area.x + 2 + app.editor.cursor_x as u16;
        let cursor_y =
            editor_content_area.y + (app.editor.cursor_y - app.editor.scroll_offset) as u16;
        if cursor_y < editor_content_area.y + editor_content_area.height {
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }
}

fn draw_find_bar(frame: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;
    let fr = &app.find_replace;

    let match_info = if fr.search_text.is_empty() {
        String::new()
    } else if fr.matches.is_empty() {
        " (no matches)".to_string()
    } else {
        format!(" ({}/{})", fr.current_match + 1, fr.matches.len())
    };

    let case_indicator = if fr.case_sensitive { " [Aa]" } else { " [.*]" };

    // Search line
    let search_label = " Find: ";
    let search_line = Line::from(vec![
        Span::styled(
            search_label,
            Style::default()
                .fg(theme.text_secondary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            fr.search_text.clone(),
            if matches!(fr.focused_field, FindReplaceField::Search) {
                Style::default().fg(theme.text_accent)
            } else {
                Style::default().fg(theme.text_primary)
            },
        ),
        Span::styled(match_info, Style::default().fg(theme.text_muted)),
        Span::styled(case_indicator, Style::default().fg(theme.text_muted)),
    ]);

    let search_area = Rect::new(area.x, area.y, area.width, 1);
    frame.render_widget(
        Paragraph::new(search_line).style(Style::default().bg(theme.bg_secondary)),
        search_area,
    );

    // Cursor in search field
    if matches!(fr.focused_field, FindReplaceField::Search) {
        let cursor_x = area.x + search_label.len() as u16 + fr.search_cursor as u16;
        if cursor_x < area.x + area.width {
            frame.set_cursor_position((cursor_x, area.y));
        }
    }

    // Replace line (if visible)
    if fr.show_replace && area.height >= 2 {
        let replace_label = " Replace: ";
        let replace_line = Line::from(vec![
            Span::styled(
                replace_label,
                Style::default()
                    .fg(theme.text_secondary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                fr.replace_text.clone(),
                if matches!(fr.focused_field, FindReplaceField::Replace) {
                    Style::default().fg(theme.text_accent)
                } else {
                    Style::default().fg(theme.text_primary)
                },
            ),
        ]);

        let replace_area = Rect::new(area.x, area.y + 1, area.width, 1);
        frame.render_widget(
            Paragraph::new(replace_line).style(Style::default().bg(theme.bg_secondary)),
            replace_area,
        );

        // Cursor in replace field
        if matches!(fr.focused_field, FindReplaceField::Replace) {
            let cursor_x = area.x + replace_label.len() as u16 + fr.replace_cursor as u16;
            if cursor_x < area.x + area.width {
                frame.set_cursor_position((cursor_x, area.y + 1));
            }
        }
    }
}

/// Determine if a line starts inside a block comment by scanning all previous lines.
fn is_in_block_comment(lines: &[String], current_line: usize) -> bool {
    let mut depth = 0i32;
    for line in lines.iter().take(current_line) {
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if i + 1 < chars.len() && chars[i] == '/' && chars[i + 1] == '*' {
                depth += 1;
                i += 2;
            } else if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '/' {
                depth = (depth - 1).max(0);
                i += 2;
            } else {
                i += 1;
            }
        }
    }
    depth > 0
}

fn highlight_sql_line<'a>(
    line: &'a str,
    theme: &Theme,
    line_number: usize,
    editor: &crate::editor::TextBuffer,
    in_active_query: bool,
    find_state: &crate::ui::FindReplaceState,
) -> Line<'a> {
    // Collect match ranges for this line for highlighting
    let match_ranges: Vec<(usize, usize, bool)> =
        if find_state.active && !find_state.search_text.is_empty() {
            find_state
                .matches
                .iter()
                .enumerate()
                .filter(|(_, &(l, _, _))| l == line_number)
                .map(|(i, &(_, start, end))| (start, end, i == find_state.current_match))
                .collect()
        } else {
            Vec::new()
        };
    let mut spans: Vec<Span> = Vec::new();
    let mut current_word = String::new();
    let mut in_string = false;
    let mut string_char = '"';
    let mut in_block_comment = is_in_block_comment(&editor.lines, line_number);

    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut i = 0;

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

    while i < len {
        let c = chars[i];
        // Compute byte index for selection check
        let byte_idx: usize = chars[..i].iter().map(|ch| ch.len_utf8()).sum();

        let is_selected = if let Some(((start_x, start_y), (end_x, end_y))) = editor.get_selection()
        {
            if start_y == end_y && line_number == start_y {
                byte_idx >= start_x && byte_idx < end_x
            } else if line_number == start_y {
                byte_idx >= start_x
            } else if line_number == end_y {
                byte_idx < end_x
            } else {
                line_number > start_y && line_number < end_y
            }
        } else {
            false
        };

        // Check if byte_idx falls inside a find match
        let find_match = match_ranges
            .iter()
            .find(|&&(start, end, _)| byte_idx >= start && byte_idx < end);

        let base_style = if is_selected {
            Style::default().bg(theme.selection)
        } else if let Some(&&(_, _, is_current)) = find_match.as_ref() {
            if is_current {
                Style::default().bg(theme.warning).fg(theme.bg_primary)
            } else {
                Style::default().bg(theme.info).fg(theme.bg_primary)
            }
        } else {
            Style::default()
        };

        // Handle block comments
        if in_block_comment {
            if i + 1 < len && c == '*' && chars[i + 1] == '/' {
                spans.push(Span::styled(
                    "*/".to_string(),
                    base_style.fg(theme.syntax_comment),
                ));
                in_block_comment = false;
                i += 2;
            } else {
                spans.push(Span::styled(
                    c.to_string(),
                    base_style.fg(theme.syntax_comment),
                ));
                i += 1;
            }
            continue;
        }

        // Start block comment
        if !in_string && i + 1 < len && c == '/' && chars[i + 1] == '*' {
            if !current_word.is_empty() {
                spans.push(create_word_span(&current_word, theme, base_style));
                current_word.clear();
            }
            spans.push(Span::styled(
                "/*".to_string(),
                base_style.fg(theme.syntax_comment),
            ));
            in_block_comment = true;
            i += 2;
            continue;
        }

        // Handle line comments
        if !in_string && i + 1 < len && c == '-' && chars[i + 1] == '-' {
            if !current_word.is_empty() {
                spans.push(create_word_span(&current_word, theme, base_style));
                current_word.clear();
            }
            let rest: String = chars[i..].iter().collect();
            spans.push(Span::styled(rest, base_style.fg(theme.syntax_comment)));
            break;
        }

        // Handle strings
        if (c == '\'' || c == '"') && !in_block_comment {
            if in_string && c == string_char {
                // Check for escaped quotes ('')
                if c == '\'' && i + 1 < len && chars[i + 1] == '\'' {
                    current_word.push(c);
                    current_word.push(c);
                    i += 2;
                    continue;
                }
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
            i += 1;
            continue;
        }

        if in_string {
            current_word.push(c);
            i += 1;
            continue;
        }

        // Handle PostgreSQL operators: ::, ->, ->>, #>, #>>, @>, <@, ?|, ?&, ||
        if i + 1 < len {
            let two_char: String = chars[i..i + 2].iter().collect();
            let is_pg_operator = matches!(
                two_char.as_str(),
                "::" | "->" | "#>" | "@>" | "<@" | "?|" | "?&" | "||" | "!=" | "<>" | ">=" | "<="
            );
            if is_pg_operator {
                if !current_word.is_empty() {
                    spans.push(create_word_span(&current_word, theme, base_style));
                    current_word.clear();
                }
                // Check for 3-char operators: ->>, #>>
                if i + 2 < len {
                    let three_char: String = chars[i..i + 3].iter().collect();
                    if matches!(three_char.as_str(), "->>" | "#>>") {
                        spans.push(Span::styled(
                            three_char,
                            base_style.fg(theme.syntax_operator),
                        ));
                        i += 3;
                        continue;
                    }
                }
                spans.push(Span::styled(two_char, base_style.fg(theme.syntax_operator)));
                i += 2;
                continue;
            }
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
                '=' | '>' | '<' | '!' | '+' | '-' | '*' | '/' | '%' | '~' | '&' | '|' | '^'
                | '#' | '@' | '?' => base_style.fg(theme.syntax_operator),
                ':' => base_style.fg(theme.syntax_operator),
                '.' => base_style.fg(theme.text_muted),
                _ => base_style.fg(theme.text_primary),
            };
            spans.push(Span::styled(c.to_string(), style));
        }

        i += 1;
    }

    // Handle remaining word
    if !current_word.is_empty() {
        let style = if in_string {
            Style::default().fg(theme.syntax_string)
        } else if in_block_comment {
            Style::default().fg(theme.syntax_comment)
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
    } else if is_sql_function(word) {
        base_style.fg(theme.syntax_function)
    } else if is_sql_type(word) {
        base_style.fg(theme.syntax_type)
    } else if word.chars().all(|c| c.is_ascii_digit() || c == '.') && !word.is_empty() {
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

    // Check if we should show a visual explain plan
    let show_plan = app.show_visual_plan
        && app
            .explain_plans
            .get(app.current_result)
            .and_then(|p| p.as_ref())
            .is_some();

    if show_plan {
        if let Some(Some(plan)) = app.explain_plans.get(app.current_result) {
            draw_explain_plan(frame, app, plan, inner);
        }
    } else if let Some(result) = app.results.get(app.current_result) {
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

fn draw_explain_plan(frame: &mut Frame, app: &App, plan: &QueryPlan, area: Rect) {
    let theme = &app.theme;
    let mut lines: Vec<Line> = Vec::new();

    // Header with total time
    let header = if let Some(total) = plan.total_time {
        format!("Query Plan (total: {})", format_duration_ms(total))
    } else {
        "Query Plan".to_string()
    };
    lines.push(Line::from(Span::styled(
        header,
        Style::default()
            .fg(theme.text_accent)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    // Render the tree
    render_plan_node(&plan.root, plan.total_time, theme, &mut lines, "", true);

    // Planning/Execution time footer
    lines.push(Line::from(""));
    if let Some(pt) = plan.planning_time {
        lines.push(Line::from(Span::styled(
            format!("Planning Time: {}", format_duration_ms(pt)),
            Style::default().fg(theme.text_secondary),
        )));
    }
    if let Some(et) = plan.execution_time {
        lines.push(Line::from(Span::styled(
            format!("Execution Time: {}", format_duration_ms(et)),
            Style::default().fg(theme.text_secondary),
        )));
    }

    // Hint
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Ctrl+E: Toggle raw/visual view",
        Style::default().fg(theme.text_muted),
    )));

    // Apply scroll
    let visible_height = area.height as usize;
    let display_lines: Vec<Line> = lines
        .into_iter()
        .skip(app.plan_scroll)
        .take(visible_height)
        .collect();

    let paragraph = Paragraph::new(display_lines);
    frame.render_widget(paragraph, area);
}

fn render_plan_node<'a>(
    node: &PlanNode,
    total_time: Option<f64>,
    theme: &'a Theme,
    lines: &mut Vec<Line<'a>>,
    prefix: &str,
    is_last: bool,
) {
    let connector = if prefix.is_empty() {
        ""
    } else if is_last {
        "└─ "
    } else {
        "├─ "
    };

    // Color based on cost
    let color_class = node_color_class(node, total_time);
    let node_color = match color_class {
        NodeColorClass::Fast => theme.success,
        NodeColorClass::Moderate => theme.warning,
        NodeColorClass::Slow => theme.error,
    };

    let check = match color_class {
        NodeColorClass::Fast => " ✓",
        NodeColorClass::Moderate => " !",
        NodeColorClass::Slow => " ✗",
    };

    // Build the node line
    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::styled(
        format!("{}{}", prefix, connector),
        Style::default().fg(theme.text_muted),
    ));
    spans.push(Span::styled(
        node.node_type.clone(),
        Style::default().fg(node_color).add_modifier(Modifier::BOLD),
    ));

    // Cost info
    if let Some((start, end)) = node.estimated_cost {
        spans.push(Span::styled(
            format!(" (cost={:.2}..{:.2}", start, end),
            Style::default().fg(theme.text_secondary),
        ));
        if let Some(rows) = node.estimated_rows {
            spans.push(Span::styled(
                format!(" rows={}", rows),
                Style::default().fg(theme.text_secondary),
            ));
        }
        spans.push(Span::styled(
            ")".to_string(),
            Style::default().fg(theme.text_secondary),
        ));
    }

    // Actual time
    if let Some((start, end)) = node.actual_time {
        spans.push(Span::styled(
            format!(" [actual: {}]", format_duration_ms(end - start)),
            Style::default().fg(node_color),
        ));
    }

    // Rows mismatch indicator
    if rows_mismatch(node) {
        if let (Some(est), Some(actual)) = (node.estimated_rows, node.actual_rows) {
            spans.push(Span::styled(
                format!(" ⚠ est={} actual={}", est, actual),
                Style::default()
                    .fg(theme.warning)
                    .add_modifier(Modifier::BOLD),
            ));
        }
    }

    spans.push(Span::styled(check, Style::default().fg(node_color)));

    lines.push(Line::from(spans));

    // Details
    let child_prefix = if prefix.is_empty() {
        "   ".to_string()
    } else if is_last {
        format!("{}   ", prefix)
    } else {
        format!("{}│  ", prefix)
    };

    for detail in &node.details {
        lines.push(Line::from(Span::styled(
            format!("{}   {}", child_prefix, detail),
            Style::default().fg(theme.text_secondary),
        )));
    }

    // Children
    for (i, child) in node.children.iter().enumerate() {
        let child_is_last = i == node.children.len() - 1;
        render_plan_node(
            child,
            total_time,
            theme,
            lines,
            &child_prefix,
            child_is_last,
        );
    }
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

fn draw_table_inspector(frame: &mut Frame, app: &App) {
    let theme = &app.theme;
    let inspector = match &app.table_inspector {
        Some(i) => i,
        None => return,
    };

    let area = frame.area();
    let width = 70.min(area.width.saturating_sub(4));
    let height = (area.height - 4).min(30);
    let x = (area.width - width) / 2;
    let y = (area.height - height) / 2;
    let dialog_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, dialog_area);

    let title = format!(
        " Table: {}.{} ",
        inspector.schema_name, inspector.table_name
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_focused))
        .title(title)
        .title_style(
            Style::default()
                .fg(theme.text_accent)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().bg(theme.bg_primary));

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let mut lines: Vec<Line> = Vec::new();

    if inspector.show_ddl {
        // DDL view
        for ddl_line in inspector.ddl.lines() {
            lines.push(Line::from(Span::styled(
                format!("  {}", ddl_line),
                Style::default().fg(theme.text_primary),
            )));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  [D] Structure  [Ctrl+C] Copy DDL  [Esc] Close",
            Style::default().fg(theme.text_muted),
        )));
    } else {
        // Structure view
        lines.push(Line::from(Span::styled(
            "  COLUMNS",
            Style::default()
                .fg(theme.text_accent)
                .add_modifier(Modifier::BOLD),
        )));

        for col in &inspector.columns {
            let pk = if col.is_primary_key { " PK" } else { "" };
            let nullable = if col.is_nullable { "NULL" } else { "NOT NULL" };
            let default = col
                .default_value
                .as_ref()
                .map(|d| format!(" DEFAULT {}", d))
                .unwrap_or_default();
            let line_text = format!(
                "  {:<20} {:<15} {:<8}{}{}",
                col.name, col.data_type, nullable, pk, default
            );
            let style = if col.is_primary_key {
                Style::default().fg(theme.warning)
            } else {
                Style::default().fg(theme.text_primary)
            };
            lines.push(Line::from(Span::styled(line_text, style)));
        }

        if !inspector.indexes.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  INDEXES",
                Style::default()
                    .fg(theme.text_accent)
                    .add_modifier(Modifier::BOLD),
            )));

            for idx in &inspector.indexes {
                let kind = if idx.is_primary {
                    "PRIMARY"
                } else if idx.is_unique {
                    "UNIQUE"
                } else {
                    ""
                };
                let line_text = format!("  {:<30} ({}) {}", idx.name, idx.columns.join(", "), kind);
                lines.push(Line::from(Span::styled(
                    line_text,
                    Style::default().fg(theme.text_primary),
                )));
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  [D] DDL  [Esc] Close",
            Style::default().fg(theme.text_muted),
        )));
    }

    // Apply scrolling
    let visible: Vec<Line> = lines
        .into_iter()
        .skip(inspector.scroll)
        .take(inner.height as usize)
        .collect();

    let paragraph = Paragraph::new(visible);
    frame.render_widget(paragraph, inner);
}

fn draw_export_picker(frame: &mut Frame, app: &App) {
    let theme = &app.theme;
    let area = frame.area();

    let row_count = app
        .results
        .get(app.current_result)
        .map(|r| r.row_count)
        .unwrap_or(0);

    let picker_width = 40.min(area.width.saturating_sub(4));
    let picker_height = (EXPORT_FORMATS.len() as u16 + 4).min(area.height.saturating_sub(4));

    let picker_x = (area.width - picker_width) / 2;
    let picker_y = (area.height - picker_height) / 2;

    let picker_area = Rect::new(picker_x, picker_y, picker_width, picker_height);
    frame.render_widget(Clear, picker_area);

    let title = format!(" Export Results ({} rows) ", row_count);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_focused))
        .title(title)
        .title_style(
            Style::default()
                .fg(theme.text_accent)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().bg(theme.bg_primary));

    let inner = block.inner(picker_area);
    frame.render_widget(block, picker_area);

    let items: Vec<ListItem> = EXPORT_FORMATS
        .iter()
        .enumerate()
        .map(|(i, fmt)| {
            let prefix = format!("  {}. ", i + 1);
            let style = if i == app.export_selected {
                Style::default()
                    .fg(theme.text_accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_primary)
            };
            ListItem::new(format!("{}{}", prefix, fmt.label())).style(style)
        })
        .collect();

    let list = List::new(items);
    let list_area = Rect::new(
        inner.x,
        inner.y,
        inner.width,
        inner.height.saturating_sub(1),
    );
    frame.render_widget(list, list_area);

    // Hint text at bottom
    let hint_area = Rect::new(
        inner.x,
        inner.y + inner.height.saturating_sub(1),
        inner.width,
        1,
    );
    let hint = Paragraph::new(" Enter: Export | 1-5: Quick select | Esc: Cancel")
        .style(Style::default().fg(theme.text_muted));
    frame.render_widget(hint, hint_area);
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
        "   Ctrl+Z         Undo",
        "   Ctrl+Shift+Z/Y Redo",
        "   Ctrl+A         Select all",
        "   Ctrl+F         Find",
        "   Ctrl+H         Find & Replace",
        "   Ctrl+Space     Trigger autocomplete",
        "   Tab            Insert spaces",
        "",
        " SIDEBAR",
        "   1/2/3          Switch tabs",
        "   Enter          Select item",
        "   ↑/↓            Navigate",
        "   Ctrl+I         Inspect table (DDL)",
        "",
        " RESULTS",
        "   Tab/Shift+Tab  Next/Prev column",
        "   Arrow keys     Navigate cells",
        "   Esc            Back to editor",
        "   Ctrl+C         Copy cell value",
        "   Ctrl+E         Toggle EXPLAIN plan view",
        "   Ctrl+S         Export results",
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

fn draw_autocomplete(frame: &mut Frame, app: &App, editor_area: Rect) {
    let theme = &app.theme;
    let ac = &app.autocomplete;

    if ac.suggestions.is_empty() {
        return;
    }

    // Position popup below the cursor
    let cursor_x = editor_area.x + app.editor.cursor_x as u16;
    let cursor_y = editor_area.y + (app.editor.cursor_y - app.editor.scroll_offset) as u16 + 1;

    let max_items = ac.suggestions.len().min(8);
    let popup_width = 35.min(editor_area.width.saturating_sub(2));
    let popup_height = (max_items as u16 + 2).min(editor_area.height.saturating_sub(2));

    // Adjust position if popup would go off screen
    let popup_x = cursor_x.min(frame.area().width.saturating_sub(popup_width));
    let popup_y = if cursor_y + popup_height > frame.area().height {
        // Show above cursor if no room below
        cursor_y.saturating_sub(popup_height + 1)
    } else {
        cursor_y
    };

    let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_focused))
        .style(Style::default().bg(theme.bg_primary));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let items: Vec<ListItem> = ac
        .suggestions
        .iter()
        .enumerate()
        .take(max_items)
        .map(|(i, suggestion)| {
            let is_selected = i == ac.selected;
            let kind_label = suggestion.kind.label();

            let text = format!(" {} {:>2} ", suggestion.text, kind_label);
            let style = if is_selected {
                Style::default()
                    .fg(theme.text_accent)
                    .bg(theme.bg_highlight)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_primary)
            };

            ListItem::new(text).style(style)
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);
}
