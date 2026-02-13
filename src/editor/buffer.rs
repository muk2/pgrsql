use arboard::Clipboard;
use std::cmp::min;

#[derive(Debug, Clone)]
pub struct TextBuffer {
    pub lines: Vec<String>,
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub selection_start: Option<(usize, usize)>,
    pub scroll_offset: usize,
    pub modified: bool,
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl TextBuffer {
    pub fn new() -> Self {
        Self {
            lines: vec![String::new()],
            cursor_x: 0,
            cursor_y: 0,
            selection_start: None,
            scroll_offset: 0,
            modified: false,
        }
    }

    pub fn from_text(text: &str) -> Self {
        let lines: Vec<String> = text.lines().map(String::from).collect();
        Self {
            lines: if lines.is_empty() {
                vec![String::new()]
            } else {
                lines
            },
            cursor_x: 0,
            cursor_y: 0,
            selection_start: None,
            scroll_offset: 0,
            modified: false,
        }
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    pub fn current_line(&self) -> &str {
        &self.lines[self.cursor_y]
    }

    pub fn current_line_mut(&mut self) -> &mut String {
        &mut self.lines[self.cursor_y]
    }

    pub fn insert_char(&mut self, c: char) {
        self.delete_selection();

        if c == '\n' {
            self.insert_newline();
        } else {
            let cx = self.cursor_x;
            let line = self.current_line_mut();
            if cx >= line.len() {
                line.push(c);
            } else {
                line.insert(cx, c);
            }
            self.cursor_x += 1;
        }
        self.modified = true;
    }

    pub fn insert_newline(&mut self) {
        let cx = self.cursor_x;
        let current_line = self.current_line_mut();
        let remainder = current_line.split_off(cx);
        self.cursor_y += 1;
        self.cursor_x = 0;
        self.lines.insert(self.cursor_y, remainder);
        self.modified = true;
    }

    pub fn insert_text(&mut self, text: &str) {
        for c in text.chars() {
            self.insert_char(c);
        }
    }

    pub fn backspace(&mut self) {
        if self.has_selection() {
            self.delete_selection();
            return;
        }

        if self.cursor_x > 0 {
            let cx = self.cursor_x;
            let line = self.current_line_mut();
            line.remove(cx - 1);
            self.cursor_x -= 1;
            self.modified = true;
        } else if self.cursor_y > 0 {
            let current_line = self.lines.remove(self.cursor_y);
            self.cursor_y -= 1;
            self.cursor_x = self.lines[self.cursor_y].len();
            self.lines[self.cursor_y].push_str(&current_line);
            self.modified = true;
        }
    }

    pub fn delete(&mut self) {
        if self.has_selection() {
            self.delete_selection();
            return;
        }

        let line_len = self.current_line().len();
        if self.cursor_x < line_len {
            let cx = self.cursor_x;
            self.current_line_mut().remove(cx);
            self.modified = true;
        } else if self.cursor_y < self.lines.len() - 1 {
            let next_line = self.lines.remove(self.cursor_y + 1);
            self.current_line_mut().push_str(&next_line);
            self.modified = true;
        }
    }

    pub fn move_left(&mut self) {
        self.clear_selection();
        if self.cursor_x > 0 {
            self.cursor_x -= 1;
        } else if self.cursor_y > 0 {
            self.cursor_y -= 1;
            self.cursor_x = self.lines[self.cursor_y].len();
        }
    }

    pub fn move_right(&mut self) {
        self.clear_selection();
        let line_len = self.current_line().len();
        if self.cursor_x < line_len {
            self.cursor_x += 1;
        } else if self.cursor_y < self.lines.len() - 1 {
            self.cursor_y += 1;
            self.cursor_x = 0;
        }
    }

    pub fn move_up(&mut self) {
        self.clear_selection();
        if self.cursor_y > 0 {
            self.cursor_y -= 1;
            self.cursor_x = min(self.cursor_x, self.lines[self.cursor_y].len());
        }
    }

    pub fn move_down(&mut self) {
        self.clear_selection();
        if self.cursor_y < self.lines.len() - 1 {
            self.cursor_y += 1;
            self.cursor_x = min(self.cursor_x, self.lines[self.cursor_y].len());
        }
    }

    pub fn move_to_line_start(&mut self) {
        self.clear_selection();
        self.cursor_x = 0;
    }

    pub fn move_to_line_end(&mut self) {
        self.clear_selection();
        self.cursor_x = self.current_line().len();
    }

    pub fn move_word_left(&mut self) {
        self.clear_selection();
        if self.cursor_x == 0 {
            if self.cursor_y > 0 {
                self.cursor_y -= 1;
                self.cursor_x = self.lines[self.cursor_y].len();
            }
            return;
        }

        let line = self.current_line();
        let chars: Vec<char> = line.chars().collect();
        let mut x = self.cursor_x;

        // Skip whitespace
        while x > 0 && chars[x - 1].is_whitespace() {
            x -= 1;
        }

        // Skip word characters
        while x > 0 && !chars[x - 1].is_whitespace() {
            x -= 1;
        }

        self.cursor_x = x;
    }

    pub fn move_word_right(&mut self) {
        self.clear_selection();
        let line_len = self.current_line().len();
        if self.cursor_x >= line_len {
            if self.cursor_y < self.lines.len() - 1 {
                self.cursor_y += 1;
                self.cursor_x = 0;
            }
            return;
        }

        let line = self.current_line();
        let chars: Vec<char> = line.chars().collect();
        let mut x = self.cursor_x;

        // Skip word characters
        while x < chars.len() && !chars[x].is_whitespace() {
            x += 1;
        }

        // Skip whitespace
        while x < chars.len() && chars[x].is_whitespace() {
            x += 1;
        }

        self.cursor_x = x;
    }

    pub fn move_to_start(&mut self) {
        self.clear_selection();
        self.cursor_x = 0;
        self.cursor_y = 0;
    }

    pub fn move_to_end(&mut self) {
        self.clear_selection();
        self.cursor_y = self.lines.len() - 1;
        self.cursor_x = self.lines[self.cursor_y].len();
    }

    // Selection methods
    pub fn start_selection(&mut self) {
        self.selection_start = Some((self.cursor_x, self.cursor_y));
    }

    pub fn clear_selection(&mut self) {
        self.selection_start = None;
    }

    pub fn has_selection(&self) -> bool {
        self.selection_start.is_some()
    }

    pub fn get_selection(&self) -> Option<((usize, usize), (usize, usize))> {
        self.selection_start.map(|start| {
            let end = (self.cursor_x, self.cursor_y);
            if start.1 < end.1 || (start.1 == end.1 && start.0 <= end.0) {
                (start, end)
            } else {
                (end, start)
            }
        })
    }

    pub fn get_selected_text(&self) -> Option<String> {
        self.get_selection().map(|(start, end)| {
            if start.1 == end.1 {
                // Same line
                self.lines[start.1][start.0..end.0].to_string()
            } else {
                let mut result = String::new();
                result.push_str(&self.lines[start.1][start.0..]);
                for y in (start.1 + 1)..end.1 {
                    result.push('\n');
                    result.push_str(&self.lines[y]);
                }
                result.push('\n');
                result.push_str(&self.lines[end.1][..end.0]);
                result
            }
        })
    }

    pub fn delete_selection(&mut self) -> bool {
        if let Some((start, end)) = self.get_selection() {
            if start.1 == end.1 {
                // Same line
                self.lines[start.1].replace_range(start.0..end.0, "");
            } else {
                // Multiple lines
                let start_part = self.lines[start.1][..start.0].to_string();
                let end_part = self.lines[end.1][end.0..].to_string();

                self.lines.drain((start.1 + 1)..=end.1);
                self.lines[start.1] = start_part + &end_part;
            }

            self.cursor_x = start.0;
            self.cursor_y = start.1;
            self.clear_selection();
            self.modified = true;
            true
        } else {
            false
        }
    }

    pub fn select_all(&mut self) {
        self.selection_start = Some((0, 0));
        self.cursor_y = self.lines.len() - 1;
        self.cursor_x = self.lines[self.cursor_y].len();
    }

    pub fn select_line(&mut self) {
        self.cursor_x = 0;
        self.selection_start = Some((0, self.cursor_y));
        self.cursor_x = self.current_line().len();
    }

    // Clipboard operations
    pub fn copy(&self) -> Option<String> {
        let text = self.get_selected_text()?;
        if let Ok(mut clipboard) = Clipboard::new() {
            let _ = clipboard.set_text(&text);
        }
        Some(text)
    }

    pub fn cut(&mut self) -> Option<String> {
        let text = self.copy()?;
        self.delete_selection();
        Some(text)
    }

    pub fn paste(&mut self) {
        if let Ok(mut clipboard) = Clipboard::new() {
            if let Ok(text) = clipboard.get_text() {
                self.delete_selection();
                self.insert_text(&text);
            }
        }
    }

    // Clear and reset
    pub fn clear(&mut self) {
        self.lines = vec![String::new()];
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.selection_start = None;
        self.scroll_offset = 0;
        self.modified = false;
    }

    pub fn set_text(&mut self, text: &str) {
        self.lines = text.lines().map(String::from).collect();
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.selection_start = None;
        self.modified = false;
    }

    // Tab handling
    pub fn insert_tab(&mut self) {
        // Insert 4 spaces
        for _ in 0..4 {
            self.insert_char(' ');
        }
    }

    // Scroll handling
    pub fn ensure_cursor_visible(&mut self, visible_height: usize) {
        if self.cursor_y < self.scroll_offset {
            self.scroll_offset = self.cursor_y;
        } else if self.cursor_y >= self.scroll_offset + visible_height {
            self.scroll_offset = self.cursor_y - visible_height + 1;
        }
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }
}
