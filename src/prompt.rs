//! A simple one-line text-prompt popup (reuses the InputField from editor).

use crossterm::event::KeyCode;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::editor::InputField;

pub struct Prompt {
    pub field: InputField,
    pub title: String,
}

impl Prompt {
    pub fn new(title: &str, default_text: &str) -> Self {
        Self {
            field: InputField::new("", default_text),
            title: title.to_string(),
        }
    }

    pub fn handle_key(&mut self, key: KeyCode) {
        self.field.handle_key(key);
    }

    pub fn value(&self) -> &str {
        &self.field.buffer
    }
}

pub fn draw_prompt(frame: &mut Frame, prompt: &Prompt, area: Rect) {
    // Dim backdrop
    let dim = Paragraph::new("")
        .style(Style::default().bg(Color::Black))
        .block(Block::default().style(Style::default().bg(Color::Black)));
    frame.render_widget(dim, area);

    let popup_w = 52_u16.min(area.width);
    let popup_h = 6_u16;
    let popup_x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let popup_y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup = Rect::new(popup_x, popup_y, popup_w, popup_h);

    let block = Block::default()
        .title(format!(" {} ", prompt.title))
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::DarkGray));
    frame.render_widget(block.clone(), popup);

    let inner = block.inner(popup);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // pad
            Constraint::Length(1), // input
            Constraint::Length(1), // help
        ])
        .split(inner);

    // Input line with cursor
    let text = if prompt.field.cursor >= prompt.field.buffer.len() {
        format!("> {} ", prompt.field.buffer)
    } else {
        let c = prompt.field.buffer[prompt.field.cursor..]
            .chars()
            .next()
            .unwrap();
        format!(
            "> {}{}{}",
            &prompt.field.buffer[..prompt.field.cursor],
            c,
            &prompt.field.buffer[prompt.field.cursor + c.len_utf8()..]
        )
    };

    let input = Paragraph::new(text)
        .style(
            Style::default()
                .fg(Color::Yellow)
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(input, rows[1]);

    let help = Paragraph::new(" Enter:confirm  Esc:cancel ")
        .style(Style::default().fg(Color::Gray).bg(Color::DarkGray));
    frame.render_widget(help, rows[2]);
}
