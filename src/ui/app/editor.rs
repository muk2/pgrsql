use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::db::execute_query;
use crate::editor::HistoryEntry;
use crate::explain::{is_explain_query, parse_explain_output};
use crate::ui::{SQL_KEYWORDS, SQL_TYPES};

use super::{App, AutocompleteSuggestion, Focus, StatusType, SuggestionKind, SQL_FUNCTIONS};

impl App {
    pub(super) async fn handle_editor_input(&mut self, key: KeyEvent) -> Result<()> {
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
}
