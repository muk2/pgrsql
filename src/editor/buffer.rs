use arboard::Clipboard;
use std::cmp::min;

#[derive(Debug, Clone, PartialEq)]
enum UndoActionType {
    Insert,
    Delete,
    Newline,
    Other,
}

#[derive(Debug, Clone)]
struct BufferSnapshot {
    lines: Vec<String>,
    cursor_x: usize,
    cursor_y: usize,
}

#[derive(Debug, Clone)]
struct UndoHistory {
    undo_stack: Vec<BufferSnapshot>,
    redo_stack: Vec<BufferSnapshot>,
    max_depth: usize,
    last_action: Option<UndoActionType>,
}

impl UndoHistory {
    fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_depth: 500,
            last_action: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TextBuffer {
    pub lines: Vec<String>,
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub selection_start: Option<(usize, usize)>,
    pub scroll_offset: usize,
    pub modified: bool,
    undo_history: UndoHistory,
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)]
impl TextBuffer {
    pub fn new() -> Self {
        Self {
            lines: vec![String::new()],
            cursor_x: 0,
            cursor_y: 0,
            selection_start: None,
            scroll_offset: 0,
            modified: false,
            undo_history: UndoHistory::new(),
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
            undo_history: UndoHistory::new(),
        }
    }

    fn snapshot(&self) -> BufferSnapshot {
        BufferSnapshot {
            lines: self.lines.clone(),
            cursor_x: self.cursor_x,
            cursor_y: self.cursor_y,
        }
    }

    fn save_undo(&mut self, action_type: UndoActionType) {
        let should_save = match (&self.undo_history.last_action, &action_type) {
            (Some(last), current) if last == current && *current == UndoActionType::Insert => false,
            (Some(last), current) if last == current && *current == UndoActionType::Delete => false,
            _ => true,
        };

        if should_save {
            let snap = self.snapshot();
            self.undo_history.undo_stack.push(snap);
            if self.undo_history.undo_stack.len() > self.undo_history.max_depth {
                self.undo_history.undo_stack.remove(0);
            }
        }

        self.undo_history.redo_stack.clear();
        self.undo_history.last_action = Some(action_type);
    }

    fn save_undo_forced(&mut self) {
        let snap = self.snapshot();
        self.undo_history.undo_stack.push(snap);
        if self.undo_history.undo_stack.len() > self.undo_history.max_depth {
            self.undo_history.undo_stack.remove(0);
        }
        self.undo_history.redo_stack.clear();
        self.undo_history.last_action = Some(UndoActionType::Other);
    }

    pub fn undo(&mut self) -> bool {
        if let Some(snap) = self.undo_history.undo_stack.pop() {
            let current = self.snapshot();
            self.undo_history.redo_stack.push(current);
            self.lines = snap.lines;
            self.cursor_x = snap.cursor_x;
            self.cursor_y = snap.cursor_y;
            self.selection_start = None;
            self.undo_history.last_action = None;
            self.modified = !self.undo_history.undo_stack.is_empty();
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self) -> bool {
        if let Some(snap) = self.undo_history.redo_stack.pop() {
            let current = self.snapshot();
            self.undo_history.undo_stack.push(current);
            self.lines = snap.lines;
            self.cursor_x = snap.cursor_x;
            self.cursor_y = snap.cursor_y;
            self.selection_start = None;
            self.undo_history.last_action = None;
            self.modified = true;
            true
        } else {
            false
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
        if self.has_selection() {
            self.save_undo_forced();
            self.delete_selection_internal();
        }

        if c == '\n' {
            self.save_undo(UndoActionType::Newline);
            self.insert_newline_internal();
        } else {
            self.save_undo(UndoActionType::Insert);
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
        self.save_undo(UndoActionType::Newline);
        self.insert_newline_internal();
    }

    fn insert_newline_internal(&mut self) {
        let cx = self.cursor_x;
        let current_line = self.current_line_mut();
        let remainder = current_line.split_off(cx);
        self.cursor_y += 1;
        self.cursor_x = 0;
        self.lines.insert(self.cursor_y, remainder);
        self.modified = true;
    }

    pub fn insert_text(&mut self, text: &str) {
        self.save_undo_forced();
        for c in text.chars() {
            if c == '\n' {
                self.insert_newline_internal();
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
        }
        self.modified = true;
    }

    pub fn backspace(&mut self) {
        if self.has_selection() {
            self.save_undo_forced();
            self.delete_selection_internal();
            return;
        }

        self.save_undo(UndoActionType::Delete);
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
            self.save_undo_forced();
            self.delete_selection_internal();
            return;
        }

        self.save_undo(UndoActionType::Delete);
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
        if self.has_selection() {
            self.save_undo_forced();
            self.delete_selection_internal()
        } else {
            false
        }
    }

    fn delete_selection_internal(&mut self) -> bool {
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
        self.save_undo_forced();
        self.delete_selection_internal();
        Some(text)
    }

    pub fn paste(&mut self) {
        if let Ok(mut clipboard) = Clipboard::new() {
            if let Ok(text) = clipboard.get_text() {
                self.save_undo_forced();
                self.delete_selection_internal();
                for c in text.chars() {
                    if c == '\n' {
                        self.insert_newline_internal();
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
                }
                self.modified = true;
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
        self.undo_history = UndoHistory::new();
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
        self.undo_history = UndoHistory::new();
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

#[cfg(test)]
mod tests {
    use super::*;

    // --- Construction ---

    #[test]
    fn test_new_buffer() {
        let buf = TextBuffer::new();
        assert_eq!(buf.lines, vec![""]);
        assert_eq!(buf.cursor_x, 0);
        assert_eq!(buf.cursor_y, 0);
        assert!(buf.selection_start.is_none());
        assert!(!buf.modified);
    }

    #[test]
    fn test_default_buffer() {
        let buf = TextBuffer::default();
        assert_eq!(buf.lines, vec![""]);
    }

    #[test]
    fn test_from_text_single_line() {
        let buf = TextBuffer::from_text("hello");
        assert_eq!(buf.lines, vec!["hello"]);
        assert_eq!(buf.cursor_x, 0);
        assert_eq!(buf.cursor_y, 0);
    }

    #[test]
    fn test_from_text_multi_line() {
        let buf = TextBuffer::from_text("line1\nline2\nline3");
        assert_eq!(buf.lines, vec!["line1", "line2", "line3"]);
    }

    #[test]
    fn test_from_text_empty() {
        let buf = TextBuffer::from_text("");
        assert_eq!(buf.lines, vec![""]);
    }

    // --- Text retrieval ---

    #[test]
    fn test_text_round_trip() {
        let original = "SELECT *\nFROM users\nWHERE id = 1";
        let buf = TextBuffer::from_text(original);
        assert_eq!(buf.text(), original);
    }

    #[test]
    fn test_current_line() {
        let buf = TextBuffer::from_text("line1\nline2");
        assert_eq!(buf.current_line(), "line1");
    }

    #[test]
    fn test_line_count() {
        let buf = TextBuffer::from_text("a\nb\nc");
        assert_eq!(buf.line_count(), 3);
    }

    // --- Insertion ---

    #[test]
    fn test_insert_char() {
        let mut buf = TextBuffer::new();
        buf.insert_char('a');
        assert_eq!(buf.text(), "a");
        assert_eq!(buf.cursor_x, 1);
        assert!(buf.modified);
    }

    #[test]
    fn test_insert_char_middle_of_line() {
        let mut buf = TextBuffer::from_text("ac");
        buf.cursor_x = 1;
        buf.insert_char('b');
        assert_eq!(buf.text(), "abc");
        assert_eq!(buf.cursor_x, 2);
    }

    #[test]
    fn test_insert_newline() {
        let mut buf = TextBuffer::from_text("hello world");
        buf.cursor_x = 5;
        buf.insert_newline();
        assert_eq!(buf.lines, vec!["hello", " world"]);
        assert_eq!(buf.cursor_y, 1);
        assert_eq!(buf.cursor_x, 0);
    }

    #[test]
    fn test_insert_text_multiline() {
        let mut buf = TextBuffer::new();
        buf.insert_text("hello\nworld");
        assert_eq!(buf.lines, vec!["hello", "world"]);
        assert_eq!(buf.cursor_y, 1);
        assert_eq!(buf.cursor_x, 5);
    }

    #[test]
    fn test_insert_tab() {
        let mut buf = TextBuffer::new();
        buf.insert_tab();
        assert_eq!(buf.text(), "    ");
        assert_eq!(buf.cursor_x, 4);
    }

    // --- Deletion ---

    #[test]
    fn test_backspace_middle() {
        let mut buf = TextBuffer::from_text("abc");
        buf.cursor_x = 2;
        buf.backspace();
        assert_eq!(buf.text(), "ac");
        assert_eq!(buf.cursor_x, 1);
    }

    #[test]
    fn test_backspace_at_start_merges_lines() {
        let mut buf = TextBuffer::from_text("line1\nline2");
        buf.cursor_y = 1;
        buf.cursor_x = 0;
        buf.backspace();
        assert_eq!(buf.lines, vec!["line1line2"]);
        assert_eq!(buf.cursor_y, 0);
        assert_eq!(buf.cursor_x, 5);
    }

    #[test]
    fn test_backspace_at_beginning_does_nothing() {
        let mut buf = TextBuffer::new();
        buf.backspace();
        assert_eq!(buf.text(), "");
    }

    #[test]
    fn test_delete_middle() {
        let mut buf = TextBuffer::from_text("abc");
        buf.cursor_x = 1;
        buf.delete();
        assert_eq!(buf.text(), "ac");
    }

    #[test]
    fn test_delete_at_end_merges_lines() {
        let mut buf = TextBuffer::from_text("line1\nline2");
        buf.cursor_x = 5;
        buf.delete();
        assert_eq!(buf.lines, vec!["line1line2"]);
    }

    #[test]
    fn test_delete_at_end_of_last_line_does_nothing() {
        let mut buf = TextBuffer::from_text("hello");
        buf.cursor_x = 5;
        buf.delete();
        assert_eq!(buf.text(), "hello");
    }

    // --- Cursor movement ---

    #[test]
    fn test_move_left() {
        let mut buf = TextBuffer::from_text("hello");
        buf.cursor_x = 3;
        buf.move_left();
        assert_eq!(buf.cursor_x, 2);
    }

    #[test]
    fn test_move_left_wraps_to_previous_line() {
        let mut buf = TextBuffer::from_text("line1\nline2");
        buf.cursor_y = 1;
        buf.cursor_x = 0;
        buf.move_left();
        assert_eq!(buf.cursor_y, 0);
        assert_eq!(buf.cursor_x, 5);
    }

    #[test]
    fn test_move_right() {
        let mut buf = TextBuffer::from_text("hello");
        buf.cursor_x = 2;
        buf.move_right();
        assert_eq!(buf.cursor_x, 3);
    }

    #[test]
    fn test_move_right_wraps_to_next_line() {
        let mut buf = TextBuffer::from_text("line1\nline2");
        buf.cursor_x = 5;
        buf.move_right();
        assert_eq!(buf.cursor_y, 1);
        assert_eq!(buf.cursor_x, 0);
    }

    #[test]
    fn test_move_up() {
        let mut buf = TextBuffer::from_text("line1\nline2");
        buf.cursor_y = 1;
        buf.cursor_x = 3;
        buf.move_up();
        assert_eq!(buf.cursor_y, 0);
        assert_eq!(buf.cursor_x, 3);
    }

    #[test]
    fn test_move_up_clamps_cursor_x() {
        let mut buf = TextBuffer::from_text("hi\nlong line");
        buf.cursor_y = 1;
        buf.cursor_x = 8;
        buf.move_up();
        assert_eq!(buf.cursor_y, 0);
        assert_eq!(buf.cursor_x, 2); // clamped to length of "hi"
    }

    #[test]
    fn test_move_down() {
        let mut buf = TextBuffer::from_text("line1\nline2");
        buf.move_down();
        assert_eq!(buf.cursor_y, 1);
    }

    #[test]
    fn test_move_to_line_start() {
        let mut buf = TextBuffer::from_text("hello");
        buf.cursor_x = 3;
        buf.move_to_line_start();
        assert_eq!(buf.cursor_x, 0);
    }

    #[test]
    fn test_move_to_line_end() {
        let mut buf = TextBuffer::from_text("hello");
        buf.move_to_line_end();
        assert_eq!(buf.cursor_x, 5);
    }

    #[test]
    fn test_move_to_start() {
        let mut buf = TextBuffer::from_text("line1\nline2");
        buf.cursor_y = 1;
        buf.cursor_x = 3;
        buf.move_to_start();
        assert_eq!(buf.cursor_y, 0);
        assert_eq!(buf.cursor_x, 0);
    }

    #[test]
    fn test_move_to_end() {
        let mut buf = TextBuffer::from_text("line1\nline2");
        buf.move_to_end();
        assert_eq!(buf.cursor_y, 1);
        assert_eq!(buf.cursor_x, 5);
    }

    // --- Word movement ---

    #[test]
    fn test_move_word_left() {
        let mut buf = TextBuffer::from_text("hello world");
        buf.cursor_x = 11;
        buf.move_word_left();
        assert_eq!(buf.cursor_x, 6);
        buf.move_word_left();
        assert_eq!(buf.cursor_x, 0);
    }

    #[test]
    fn test_move_word_right() {
        let mut buf = TextBuffer::from_text("hello world");
        buf.move_word_right();
        assert_eq!(buf.cursor_x, 6);
        buf.move_word_right();
        assert_eq!(buf.cursor_x, 11);
    }

    #[test]
    fn test_move_word_left_across_lines() {
        let mut buf = TextBuffer::from_text("line1\nline2");
        buf.cursor_y = 1;
        buf.cursor_x = 0;
        buf.move_word_left();
        assert_eq!(buf.cursor_y, 0);
        assert_eq!(buf.cursor_x, 5);
    }

    #[test]
    fn test_move_word_right_across_lines() {
        let mut buf = TextBuffer::from_text("line1\nline2");
        buf.cursor_x = 5;
        buf.move_word_right();
        assert_eq!(buf.cursor_y, 1);
        assert_eq!(buf.cursor_x, 0);
    }

    // --- Selection ---

    #[test]
    fn test_selection_start_and_get() {
        let mut buf = TextBuffer::from_text("hello world");
        buf.cursor_x = 2;
        buf.start_selection();
        buf.cursor_x = 7;
        let sel = buf.get_selection().unwrap();
        assert_eq!(sel, ((2, 0), (7, 0)));
    }

    #[test]
    fn test_get_selected_text_same_line() {
        let mut buf = TextBuffer::from_text("hello world");
        buf.cursor_x = 0;
        buf.start_selection();
        buf.cursor_x = 5;
        assert_eq!(buf.get_selected_text().unwrap(), "hello");
    }

    #[test]
    fn test_get_selected_text_multi_line() {
        let mut buf = TextBuffer::from_text("line1\nline2\nline3");
        buf.cursor_x = 3;
        buf.cursor_y = 0;
        buf.start_selection();
        buf.cursor_y = 2;
        buf.cursor_x = 2;
        let text = buf.get_selected_text().unwrap();
        assert_eq!(text, "e1\nline2\nli");
    }

    #[test]
    fn test_select_all() {
        let mut buf = TextBuffer::from_text("line1\nline2");
        buf.select_all();
        assert_eq!(buf.selection_start, Some((0, 0)));
        assert_eq!(buf.cursor_y, 1);
        assert_eq!(buf.cursor_x, 5);
    }

    #[test]
    fn test_select_line() {
        let mut buf = TextBuffer::from_text("hello world");
        buf.select_line();
        assert_eq!(buf.selection_start, Some((0, 0)));
        assert_eq!(buf.cursor_x, 11);
    }

    #[test]
    fn test_delete_selection_same_line() {
        let mut buf = TextBuffer::from_text("hello world");
        buf.cursor_x = 0;
        buf.start_selection();
        buf.cursor_x = 6;
        buf.delete_selection();
        assert_eq!(buf.text(), "world");
        assert_eq!(buf.cursor_x, 0);
    }

    #[test]
    fn test_delete_selection_multi_line() {
        let mut buf = TextBuffer::from_text("line1\nline2\nline3");
        buf.cursor_x = 3;
        buf.cursor_y = 0;
        buf.start_selection();
        buf.cursor_y = 2;
        buf.cursor_x = 3;
        buf.delete_selection();
        assert_eq!(buf.text(), "line3");
    }

    #[test]
    fn test_clear_selection() {
        let mut buf = TextBuffer::new();
        buf.start_selection();
        assert!(buf.has_selection());
        buf.clear_selection();
        assert!(!buf.has_selection());
    }

    // --- Clear / Set ---

    #[test]
    fn test_clear() {
        let mut buf = TextBuffer::from_text("something");
        buf.cursor_x = 5;
        buf.modified = true;
        buf.clear();
        assert_eq!(buf.text(), "");
        assert_eq!(buf.cursor_x, 0);
        assert_eq!(buf.cursor_y, 0);
        assert!(!buf.modified);
    }

    #[test]
    fn test_set_text() {
        let mut buf = TextBuffer::from_text("old");
        buf.set_text("new\ncontent");
        assert_eq!(buf.lines, vec!["new", "content"]);
        assert_eq!(buf.cursor_x, 0);
        assert_eq!(buf.cursor_y, 0);
        assert!(!buf.modified);
    }

    // --- Scroll ---

    #[test]
    fn test_ensure_cursor_visible_scrolls_down() {
        let mut buf = TextBuffer::from_text("1\n2\n3\n4\n5\n6\n7\n8\n9\n10");
        buf.cursor_y = 8;
        buf.ensure_cursor_visible(5);
        assert_eq!(buf.scroll_offset, 4);
    }

    #[test]
    fn test_ensure_cursor_visible_scrolls_up() {
        let mut buf = TextBuffer::from_text("1\n2\n3\n4\n5");
        buf.scroll_offset = 3;
        buf.cursor_y = 1;
        buf.ensure_cursor_visible(5);
        assert_eq!(buf.scroll_offset, 1);
    }

    // --- Edge cases ---

    #[test]
    fn test_backspace_with_selection() {
        let mut buf = TextBuffer::from_text("hello world");
        buf.cursor_x = 0;
        buf.start_selection();
        buf.cursor_x = 5;
        buf.backspace();
        assert_eq!(buf.text(), " world");
    }

    #[test]
    fn test_delete_with_selection() {
        let mut buf = TextBuffer::from_text("hello world");
        buf.cursor_x = 6;
        buf.start_selection();
        buf.cursor_x = 11;
        buf.delete();
        assert_eq!(buf.text(), "hello ");
    }

    #[test]
    fn test_insert_char_replaces_selection() {
        let mut buf = TextBuffer::from_text("hello");
        buf.cursor_x = 0;
        buf.start_selection();
        buf.cursor_x = 5;
        buf.insert_char('X');
        assert_eq!(buf.text(), "X");
    }

    // --- Undo / Redo ---

    #[test]
    fn test_undo_insert_char() {
        let mut buf = TextBuffer::new();
        buf.insert_char('a');
        buf.insert_char('b');
        buf.insert_char('c');
        assert_eq!(buf.text(), "abc");
        // Consecutive inserts are grouped, so one undo reverts all
        buf.undo();
        assert_eq!(buf.text(), "");
    }

    #[test]
    fn test_undo_newline_creates_new_group() {
        let mut buf = TextBuffer::new();
        buf.insert_char('a');
        buf.insert_char('b');
        buf.insert_newline();
        buf.insert_char('c');
        assert_eq!(buf.text(), "ab\nc");
        // Undo the 'c' (insert group)
        buf.undo();
        assert_eq!(buf.text(), "ab\n");
        // Undo the newline
        buf.undo();
        assert_eq!(buf.text(), "ab");
        // Undo the 'ab' (insert group)
        buf.undo();
        assert_eq!(buf.text(), "");
    }

    #[test]
    fn test_redo() {
        let mut buf = TextBuffer::new();
        buf.insert_char('a');
        buf.insert_char('b');
        assert_eq!(buf.text(), "ab");
        buf.undo();
        assert_eq!(buf.text(), "");
        buf.redo();
        assert_eq!(buf.text(), "ab");
    }

    #[test]
    fn test_redo_cleared_on_new_edit() {
        let mut buf = TextBuffer::new();
        buf.insert_char('a');
        buf.undo();
        assert_eq!(buf.text(), "");
        buf.insert_char('b');
        assert!(!buf.redo()); // redo stack should be cleared
        assert_eq!(buf.text(), "b");
    }

    #[test]
    fn test_undo_backspace() {
        let mut buf = TextBuffer::from_text("abc");
        buf.cursor_x = 3;
        buf.backspace();
        buf.backspace();
        assert_eq!(buf.text(), "a");
        buf.undo();
        assert_eq!(buf.text(), "abc");
    }

    #[test]
    fn test_undo_delete_selection() {
        let mut buf = TextBuffer::from_text("hello world");
        buf.cursor_x = 0;
        buf.start_selection();
        buf.cursor_x = 5;
        buf.delete_selection();
        assert_eq!(buf.text(), " world");
        buf.undo();
        assert_eq!(buf.text(), "hello world");
    }

    #[test]
    fn test_undo_on_empty_returns_false() {
        let mut buf = TextBuffer::new();
        assert!(!buf.undo());
    }

    #[test]
    fn test_redo_on_empty_returns_false() {
        let mut buf = TextBuffer::new();
        assert!(!buf.redo());
    }

    #[test]
    fn test_undo_insert_text() {
        let mut buf = TextBuffer::new();
        buf.insert_text("hello\nworld");
        assert_eq!(buf.text(), "hello\nworld");
        buf.undo();
        assert_eq!(buf.text(), "");
    }

    #[test]
    fn test_set_text_clears_undo() {
        let mut buf = TextBuffer::new();
        buf.insert_char('a');
        buf.set_text("new content");
        assert!(!buf.undo()); // undo stack should be empty
    }

    #[test]
    fn test_clear_clears_undo() {
        let mut buf = TextBuffer::from_text("content");
        buf.insert_char('x');
        buf.clear();
        assert!(!buf.undo()); // undo stack should be empty
    }

    #[test]
    fn test_undo_cursor_position_restored() {
        let mut buf = TextBuffer::new();
        buf.insert_char('a');
        buf.insert_char('b');
        assert_eq!(buf.cursor_x, 2);
        buf.undo();
        assert_eq!(buf.cursor_x, 0);
    }

    #[test]
    fn test_multiple_undo_redo_cycles() {
        let mut buf = TextBuffer::new();
        buf.insert_text("first");
        buf.insert_newline();
        buf.insert_text("second");
        // undo insert "second"
        buf.undo();
        assert_eq!(buf.text(), "first\n");
        // undo newline
        buf.undo();
        assert_eq!(buf.text(), "first");
        // redo newline
        buf.redo();
        assert_eq!(buf.text(), "first\n");
        // redo insert "second"
        buf.redo();
        assert_eq!(buf.text(), "first\nsecond");
    }
}
