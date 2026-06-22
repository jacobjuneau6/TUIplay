extern crate anyhow;
extern crate cpal;
extern crate crossterm;
extern crate lofty;
extern crate ogg;
extern crate opus;
extern crate ratatui;
extern crate rusqlite;
extern crate serde;
extern crate serde_json;
extern crate symphonia;
extern crate tokio;
extern crate walkdir;

use std::fs;
use std::io;
use std::path::PathBuf;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::Terminal;

mod app;
mod cd_writer;
mod db;
mod editor;
mod library;
mod metadata;
mod models;
mod ogg_utils;
mod player;
mod prompt;
mod scanner;
mod ui;

use app::App;

fn main() -> Result<()> {
    // ── Ensure the Music folder exists ────────────────────────────────────
    let music_dir = PathBuf::from("/home/jacob/Music");
    fs::create_dir_all(&music_dir)?;

    let db_path = music_dir.join("music.db");
    let db_path_str = db_path.to_string_lossy().to_string();

    let conn = db::init_db(&db_path_str)?;
    scanner::scan_folder(&conn, music_dir.to_str().unwrap())?;
    let tracks = library::load_tracks(&conn);
    // conn dropped — App opens its own short-lived connections for writes.

    if tracks.is_empty() {
        println!("No tracks found in {}", music_dir.display());
        return Ok(());
    }

    println!("Loaded {} tracks.  Starting TUI…", tracks.len());

    // ── Terminal setup ────────────────────────────────────────────────────
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(tracks, &db_path_str, music_dir);

    let res = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    res
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    loop {
        app.check_auto_advance();
        terminal.draw(|f| ui::draw(f, app))?;

        if !event::poll(std::time::Duration::from_millis(100))? {
            continue;
        }

        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Release {
                continue;
            }

            // ── Prompt mode ───────────────────────────────────────────────
            if let Some(ref mut prompt) = app.prompt {
                match key.code {
                    KeyCode::Esc => app.cancel_prompt(),
                    KeyCode::Enter => app.confirm_prompt(),
                    _ => prompt.handle_key(key.code),
                }
                continue;
            }

            // ── Editor mode ───────────────────────────────────────────────
            if let Some(ref mut editor) = app.editor {
                match key.code {
                    KeyCode::Esc => app.cancel_editor(),
                    KeyCode::Enter => app.save_metadata(),
                    KeyCode::Tab => {
                        if key.modifiers.contains(KeyModifiers::SHIFT) {
                            editor.prev_field();
                        } else {
                            editor.next_field();
                        }
                    }
                    KeyCode::Up => editor.prev_field(),
                    KeyCode::Down => editor.next_field(),
                    _ => editor.handle_key(key.code),
                }
                continue;
            }

            // ── Normal mode ───────────────────────────────────────────────
            match key.code {
                KeyCode::Char('q') => break,

                // Navigation
                KeyCode::Char('j') | KeyCode::Down => app.select_next(),
                KeyCode::Char('k') | KeyCode::Up => app.select_prev(),
                KeyCode::Tab => app.switch_focus(),

                // Playback
                KeyCode::Enter => app.play_selected(),
                KeyCode::Char(' ') => app.toggle_pause(),
                KeyCode::Char('s') => app.stop(),
                KeyCode::Char('n') => app.play_next_in_queue(),

                // Queue
                KeyCode::Char('a') => {
                    if app.focus == app::Panel::Library {
                        app.queue_add();
                    }
                }
                KeyCode::Char('A') => app.queue_add_all(),
                KeyCode::Char('d') => app.queue_remove_selected(),
                KeyCode::Char('C') => app.queue_clear(),

                // Metadata
                KeyCode::Char('e') => app.open_editor(),

                // Playlist save/load — open a name prompt
                KeyCode::Char('S') => app.open_prompt(app::PromptAction::SavePlaylist, "queue"),
                KeyCode::Char('L') => app.open_prompt(app::PromptAction::LoadPlaylist, "queue"),

                // CD export / burn
                KeyCode::Char('x') => app.open_prompt(app::PromptAction::CdExport, "my-mix"),
                KeyCode::Char('b') => app.burn_last_export(None),
                KeyCode::Char('c') => {
                    app.open_prompt(
                        app::PromptAction::CopyToFolder,
                        "/home/jacob/Music/copy",
                    );
                }

                _ => {}
            }
        }
    }

    Ok(())
}
