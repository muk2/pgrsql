use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{App, ExportFormat, Focus, StatusType, EXPORT_FORMATS};

impl App {
    pub(super) async fn handle_results_input(&mut self, key: KeyEvent) -> Result<()> {
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

    pub(super) async fn handle_table_inspector_input(
        &mut self,
        key: KeyEvent,
    ) -> Result<()> {
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

    pub(super) async fn handle_export_input(&mut self, key: KeyEvent) -> Result<()> {
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
}
