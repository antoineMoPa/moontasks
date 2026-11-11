//! This module holds a line of text being typed: a card's title, its tags, a line for an agent
//! or the wire, the board's filter.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Says what a key did to the line.
#[derive(Debug, PartialEq, Eq)]
pub enum LineEdit {
    /// Means the line is still being typed.
    Editing,
    /// Means enter was pressed: the line is done.
    Submitted,
    /// Means escape was pressed: the line is let go.
    Cancelled,
}

#[derive(Default)]
pub struct LineInput {
    pub text: String,
    /// Holds the cursor, in characters from the start.
    cursor: usize,
}

impl LineInput {
    pub fn holding(text: &str) -> Self {
        Self {
            text: text.to_string(),
            cursor: text.chars().count(),
        }
    }

    pub fn cursor_in_characters(&self) -> usize {
        self.cursor
    }

    fn byte_at(&self, characters: usize) -> usize {
        self.text
            .char_indices()
            .nth(characters)
            .map_or(self.text.len(), |(index, _)| index)
    }

    pub fn type_text(&mut self, typed: &str) {
        let at = self.byte_at(self.cursor);
        let typed: String = typed
            .chars()
            .filter(|character| *character != '\n')
            .collect();
        self.text.insert_str(at, &typed);
        self.cursor += typed.chars().count();
    }

    pub fn press(&mut self, event: &KeyEvent) -> LineEdit {
        let ctrl = event.modifiers.contains(KeyModifiers::CONTROL);
        match event.code {
            KeyCode::Enter => return LineEdit::Submitted,
            KeyCode::Esc => return LineEdit::Cancelled,
            KeyCode::Char('c') if ctrl => return LineEdit::Cancelled,
            KeyCode::Char('a') if ctrl => self.cursor = 0,
            KeyCode::Char('e') if ctrl => self.cursor = self.text.chars().count(),
            KeyCode::Char('u') if ctrl => {
                let at = self.byte_at(self.cursor);
                self.text.replace_range(..at, "");
                self.cursor = 0;
            }
            KeyCode::Char('w') if ctrl => self.delete_the_word_before_the_cursor(),
            KeyCode::Char(character) if !ctrl => self.type_text(&character.to_string()),
            KeyCode::Backspace if self.cursor > 0 => {
                let (from, to) = (self.byte_at(self.cursor - 1), self.byte_at(self.cursor));
                self.text.replace_range(from..to, "");
                self.cursor -= 1;
            }
            KeyCode::Delete if self.cursor < self.text.chars().count() => {
                let (from, to) = (self.byte_at(self.cursor), self.byte_at(self.cursor + 1));
                self.text.replace_range(from..to, "");
            }
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.text.chars().count()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.text.chars().count(),
            _ => {}
        }
        LineEdit::Editing
    }

    fn delete_the_word_before_the_cursor(&mut self) {
        let before: Vec<char> = self.text.chars().take(self.cursor).collect();
        let mut start = before.len();
        while start > 0 && before[start - 1] == ' ' {
            start -= 1;
        }
        while start > 0 && before[start - 1] != ' ' {
            start -= 1;
        }
        let (from, to) = (self.byte_at(start), self.byte_at(self.cursor));
        self.text.replace_range(from..to, "");
        self.cursor = start;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn typing_and_erasing_moves_the_cursor_with_the_text() {
        let mut line = LineInput::default();
        for character in "fix thé races".chars() {
            line.press(&key(KeyCode::Char(character)));
        }
        line.press(&key(KeyCode::Backspace));
        assert_eq!(line.text, "fix thé race");
        line.press(&key(KeyCode::Home));
        line.press(&key(KeyCode::Char('>')));
        assert_eq!(line.text, ">fix thé race");
        assert_eq!(line.press(&key(KeyCode::Enter)), LineEdit::Submitted);
    }

    #[test]
    fn ctrl_w_takes_the_last_word_back() {
        let mut line = LineInput::holding("start with the tests");
        line.press(&KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL));
        assert_eq!(line.text, "start with the ");
    }
}
