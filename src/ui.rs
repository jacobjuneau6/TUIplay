use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::app::{App, Panel};
use crate::editor;
use crate::player::PlaybackState;

pub fn draw(frame: &mut Frame, app: &App) {
    let status = app.player.status.lock().unwrap().clone();

    let has_message = app.message.is_some();
    let bottom_height = if has_message { 3 } else { 2 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(bottom_height),
        ])
        .split(frame.area());

    // ── Header ────────────────────────────────────────────────────────────
    let header = Paragraph::new("TUIplay")
        .style(Style::default().fg(Color::Cyan))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(header, chunks[0]);

    // ── Main area ─────────────────────────────────────────────────────────
    let main = chunks[1];
    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
        .split(main);

    draw_library(frame, app, &status, panels[0]);
    draw_queue(frame, app, &status, panels[1]);

    // ── Status / message ──────────────────────────────────────────────────
    draw_status(frame, app, &status, chunks[2], has_message);

    // ── Editor overlay ────────────────────────────────────────────────────
    if let Some(ref ed) = app.editor {
        editor::draw_editor(frame, ed, frame.area());
    }

    // ── Prompt overlay ────────────────────────────────────────────────────
    if let Some(ref prompt) = app.prompt {
        crate::prompt::draw_prompt(frame, prompt, frame.area());
    }
}

fn draw_library(
    frame: &mut Frame,
    app: &App,
    status: &crate::player::PlayerStatus,
    area: Rect,
) {
    let focused = app.focus == Panel::Library;

    let items: Vec<ListItem> = app
        .tracks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let prefix = match (&status.state, status.track_index) {
                (PlaybackState::Playing, Some(idx)) if idx == i => "▶ ",
                (PlaybackState::Paused, Some(idx)) if idx == i => "⏸ ",
                _ => "  ",
            };
            let style = if focused && i == app.selected {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            // Show duration if available
            let dur_str = t
                .duration
                .map(|d| format!(" [{}]", fmt_secs(d)))
                .unwrap_or_default();
            ListItem::new(format!("{}{} — {}{}", prefix, t.artist, t.title, dur_str))
                .style(style)
        })
        .collect();

    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    let title = if focused { " ▶ Library " } else { "   Library " };
    let count = format!(" {} tracks ", app.tracks.len());

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(title)
                .title_bottom(count),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    frame.render_widget(list, area);
}

fn draw_queue(
    frame: &mut Frame,
    app: &App,
    status: &crate::player::PlayerStatus,
    area: Rect,
) {
    let focused = app.focus == Panel::Queue;

    let items: Vec<ListItem> = app
        .queue
        .iter()
        .enumerate()
        .map(|(qi, &ti)| {
            let t = &app.tracks[ti];
            let prefix = match (&status.state, status.track_index) {
                (PlaybackState::Playing, Some(idx)) if idx == ti => String::from("▶ "),
                (PlaybackState::Paused, Some(idx)) if idx == ti => String::from("⏸ "),
                _ => format!("{}. ", qi + 1),
            };
            let style = if focused && qi == app.queue_selected {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let display = if t.artist != "Unknown" {
                format!("{}{} — {}", prefix, t.artist, t.title)
            } else {
                format!("{}{}", prefix, t.title)
            };
            let max_w = area.width.saturating_sub(4) as usize;
            let short: String = if display.len() > max_w {
                display.chars().take(max_w.saturating_sub(3)).chain("...".chars()).collect()
            } else {
                display
            };
            ListItem::new(short).style(style)
        })
        .collect();

    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    let title = if focused { " ▶ Queue " } else { "   Queue " };
    let count = format!(" {} items ", app.queue.len());

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(title)
                .title_bottom(count),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    frame.render_widget(list, area);
}

fn draw_status(
    frame: &mut Frame,
    app: &App,
    status: &crate::player::PlayerStatus,
    area: Rect,
    has_message: bool,
) {
    if has_message {
        let msg_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(area);

        let left = status_line(status, app);
        let bar = Paragraph::new(format!(
            "{}    Tab:switch  Enter:play  Space:pause  n:next  e:edit  a:add  S:save  L:load  x:CD  b:burn  c:copy  q:quit",
            left
        ))
        .style(Style::default().fg(Color::Black).bg(Color::Gray));
        frame.render_widget(bar, msg_chunks[0]);

        let msg = Paragraph::new(app.message.as_deref().unwrap_or(""))
            .style(Style::default().fg(Color::White).bg(Color::Red));
        frame.render_widget(msg, msg_chunks[1]);
    } else {
        let left = status_line(status, app);
        let bar = Paragraph::new(format!(
            "{}    Tab:switch  Enter:play  Space:pause  n:next  e:edit  a:add  S:save  L:load  x:CD  b:burn  c:copy  q:quit",
            left
        ))
        .style(Style::default().fg(Color::Black).bg(Color::Gray));
        frame.render_widget(bar, area);
    }
}

fn status_line(status: &crate::player::PlayerStatus, app: &App) -> String {
    let (elapsed, total) = app.playback_times();
    let time_str = match total {
        Some(t) => format!(" {} / {} ", fmt_secs_float(elapsed), fmt_secs(t)),
        None if elapsed > 1.0 => format!(" {} ", fmt_secs_float(elapsed)),
        _ => String::new(),
    };

    match (&status.state, status.track_index) {
        (PlaybackState::Playing, Some(idx)) => {
            let t = &app.tracks[idx];
            format!("▶ {} — {}  {}", t.artist, t.title, time_str)
        }
        (PlaybackState::Paused, Some(idx)) => {
            let t = &app.tracks[idx];
            format!("⏸ {} — {}  {}", t.artist, t.title, time_str)
        }
        _ => format!("■  Stopped{}", if !time_str.is_empty() { " (resume available)" } else { "" }),
    }
}

fn fmt_secs(secs: u64) -> String {
    let m = secs / 60;
    let s = secs % 60;
    format!("{}:{:02}", m, s)
}

fn fmt_secs_float(secs: f64) -> String {
    let s = secs as u64;
    let m = s / 60;
    let s = s % 60;
    format!("{}:{:02}", m, s)
}
