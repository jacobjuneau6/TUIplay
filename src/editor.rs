use crossterm::event::KeyCode;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::models::Track;

// ── Input field ───────────────────────────────────────────────────────────
//
// `cursor` is always a valid UTF-8 **byte** offset into `buffer`.

pub struct InputField {
    pub buffer: String,
    pub cursor: usize,   // byte position
    pub label: String,
}

impl InputField {
    pub fn new(label: &str, initial: &str) -> Self {
        let buf = initial.to_string();
        let cursor = buf.len(); // byte len, always on a code-point boundary
        Self {
            buffer: buf,
            cursor,
            label: label.to_string(),
        }
    }

    /// Byte length of the character *at* `self.cursor` (0 when at end).
    fn char_len_at_cursor(&self) -> usize {
        self.buffer[self.cursor..]
            .chars()
            .next()
            .map(|c| c.len_utf8())
            .unwrap_or(0)
    }

    /// Byte position of the code-point *before* `self.cursor`.
    fn prev_char_boundary(&self) -> Option<usize> {
        if self.cursor == 0 {
            return None;
        }
        // Walk backwards through the prefix to find the last char boundary.
        self.buffer[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
    }

    pub fn handle_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char(c) => {
                self.buffer.insert(self.cursor, c);
                self.cursor += c.len_utf8();
            }
            KeyCode::Backspace => {
                if let Some(prev) = self.prev_char_boundary() {
                    // Remove the code-point starting at `prev`.
                    let end = self.cursor;
                    self.buffer.drain(prev..end);
                    self.cursor = prev;
                }
            }
            KeyCode::Delete => {
                if self.cursor < self.buffer.len() {
                    let len = self.char_len_at_cursor();
                    if len > 0 {
                        self.buffer.drain(self.cursor..self.cursor + len);
                    }
                }
            }
            KeyCode::Left => {
                if let Some(prev) = self.prev_char_boundary() {
                    self.cursor = prev;
                }
            }
            KeyCode::Right => {
                if self.cursor < self.buffer.len() {
                    let len = self.char_len_at_cursor();
                    self.cursor += len;
                }
            }
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.buffer.len(),
            _ => {}
        }
    }
}

// ── Metadata editor ───────────────────────────────────────────────────────

pub struct MetadataEditor {
    pub fields: Vec<InputField>,
    pub focused_field: usize,
    pub track_index: usize,
    pub track_id: i64,
    pub file_name: String,
}

impl MetadataEditor {
    pub fn from_track(track: &Track, index: usize) -> Self {
        // Extract a short file name for display.
        let file_name = std::path::Path::new(&track.path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&track.path)
            .to_string();

        Self {
            fields: vec![
                InputField::new("Title", &track.title),
                InputField::new("Artist", &track.artist),
                InputField::new("Album", &track.album),
                InputField::new("Genre", &track.genre),
            ],
            focused_field: 0,
            track_index: index,
            track_id: track.id,
            file_name,
        }
    }

    pub fn next_field(&mut self) {
        if self.focused_field + 1 < self.fields.len() {
            self.focused_field += 1;
        }
    }

    pub fn prev_field(&mut self) {
        if self.focused_field > 0 {
            self.focused_field -= 1;
        }
    }

    pub fn handle_key(&mut self, key: KeyCode) {
        self.fields[self.focused_field].handle_key(key);
    }

    /// Returns (title, artist, album, genre)
    pub fn values(&self) -> (&str, &str, &str, &str) {
        (
            &self.fields[0].buffer,
            &self.fields[1].buffer,
            &self.fields[2].buffer,
            &self.fields[3].buffer,
        )
    }
}

// ── Rendering ─────────────────────────────────────────────────────────────

pub fn draw_editor(frame: &mut Frame, editor: &MetadataEditor, area: Rect) {
    // Dim background
    let dim = Paragraph::new("")
        .style(Style::default().bg(Color::Black))
        .block(Block::default().style(Style::default().bg(Color::Black)));
    frame.render_widget(dim, area);

    // Popup dimensions — enough for filename + 4 fields + help
    let popup_w = 56_u16.min(area.width);
    let popup_h = (editor.fields.len() as u16 + 9).min(area.height);
    let popup_x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let popup_y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup = Rect::new(popup_x, popup_y, popup_w, popup_h);

    let block = Block::default()
        .title(" Edit Metadata ")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::DarkGray));
    frame.render_widget(block.clone(), popup);

    let inner = block.inner(popup);

    let mut constraints: Vec<Constraint> = Vec::new();
    constraints.push(Constraint::Length(1)); // top pad
    constraints.push(Constraint::Length(2)); // filename
    constraints.push(Constraint::Length(1)); // spacing
    for _ in 0..editor.fields.len() {
        constraints.push(Constraint::Length(2));
    }
    constraints.push(Constraint::Length(1)); // help line

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    // ── File name ─────────────────────────────────────────────────────────
    let fname = Paragraph::new(format!("  File: {}", editor.file_name))
        .style(Style::default().fg(Color::White).bg(Color::DarkGray));
    frame.render_widget(fname, rows[1]);

    // ── Fields ────────────────────────────────────────────────────────────
    for (i, field) in editor.fields.iter().enumerate() {
        let row = rows[i + 3]; // skip pad + filename + spacer
        let field_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(9), Constraint::Min(1)])
            .split(row);

        // Label
        let label = Paragraph::new(format!("  {}:", field.label))
            .style(Style::default().fg(Color::Cyan));
        frame.render_widget(label, field_chunks[0]);

        // Value with cursor.
        // `cursor` is a byte offset; it is always on a code-point boundary.
        let text = if i == editor.focused_field {
            if field.cursor >= field.buffer.len() {
                // Cursor parked at the very end — show a trailing space as
                // the "fake" cursor so the user sees where they are.
                format!("{} ", field.buffer)
            } else {
                let before = &field.buffer[..field.cursor];
                let c = field.buffer[field.cursor..]
                    .chars()
                    .next()
                    .unwrap(); // safe: cursor < buffer.len()
                let cur_char = c.to_string();
                let after =
                    &field.buffer[field.cursor + c.len_utf8()..];
                format!("{}{}{}", before, cur_char, after)
            }
        } else {
            field.buffer.clone()
        };

        let style = if i == editor.focused_field {
            Style::default()
                .fg(Color::Yellow)
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White).bg(Color::DarkGray)
        };

        let display = format!("[{}]", text);
        let p = Paragraph::new(display).style(style);
        frame.render_widget(p, field_chunks[1]);
    }

    // ── Help line ─────────────────────────────────────────────────────────
    let help = Paragraph::new(" Enter:save  Esc:cancel  Tab/Shift+Tab:next field ")
        .style(Style::default().fg(Color::Gray).bg(Color::DarkGray));
    frame.render_widget(help, rows[rows.len() - 1]);
}
