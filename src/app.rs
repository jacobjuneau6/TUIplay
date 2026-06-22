use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::models::Track;
use crate::player::{PlaybackState, Player};
use crate::editor::MetadataEditor;
use crate::prompt::Prompt;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Panel {
    Library,
    Queue,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PromptAction {
    SavePlaylist,
    LoadPlaylist,
    CdExport,
    CopyToFolder,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistFile {
    pub name: String,
    pub tracks: Vec<String>, // absolute file paths
}

pub struct App {
    pub tracks: Vec<Track>,
    pub selected: usize,
    pub queue: Vec<usize>,
    pub queue_selected: usize,
    pub focus: Panel,
    pub player: Player,
    pub message: Option<String>,
    pub editor: Option<MetadataEditor>,
    pub prev_player_state: PlaybackState,
    pub db_path: String,
    /// Base folder for playlists and the DB (the user's Music folder).
    pub music_dir: PathBuf,
    /// The track we last stopped (so we can offer resume).
    pub last_played_path: Option<String>,
    pub prompt: Option<Prompt>,
    pub prompt_action: PromptAction,
}

impl App {
    pub fn new(tracks: Vec<Track>, db_path: &str, music_dir: PathBuf) -> Self {
        Self {
            tracks,
            selected: 0,
            queue: Vec::new(),
            queue_selected: 0,
            focus: Panel::Library,
            player: Player::new(),
            message: None,
            editor: None,
            prev_player_state: PlaybackState::Stopped,
            db_path: db_path.to_string(),
            music_dir,
            last_played_path: None,
            prompt: None,
            prompt_action: PromptAction::SavePlaylist,
        }
    }

    // ── Navigation ────────────────────────────────────────────────────────

    fn len(&self) -> usize {
        match self.focus {
            Panel::Library => self.tracks.len(),
            Panel::Queue => self.queue.len(),
        }
    }

    pub fn select_next(&mut self) {
        let len = self.len();
        if len == 0 { return; }
        let sel = self.selection_mut();
        *sel = (*sel + 1) % len;
    }

    pub fn select_prev(&mut self) {
        let len = self.len();
        if len == 0 { return; }
        let sel = self.selection_mut();
        if *sel == 0 {
            *sel = len - 1;
        } else {
            *sel -= 1;
        }
    }

    fn selection_mut(&mut self) -> &mut usize {
        match self.focus {
            Panel::Library => &mut self.selected,
            Panel::Queue => &mut self.queue_selected,
        }
    }

    pub fn selected_track_index(&self) -> Option<usize> {
        match self.focus {
            Panel::Library => Some(self.selected).filter(|_| !self.tracks.is_empty()),
            Panel::Queue => self.queue.get(self.queue_selected).copied(),
        }
    }

    pub fn switch_focus(&mut self) {
        self.focus = match self.focus {
            Panel::Library => Panel::Queue,
            Panel::Queue => Panel::Library,
        };
    }

    // ── Playback ──────────────────────────────────────────────────────────

    pub fn play_selected(&mut self) {
        self.message = None;
        let idx = match self.selected_track_index() {
            Some(i) => i,
            None => return,
        };
        let track = &self.tracks[idx];
        let resume = self.player.take_resume(&track.path);
        let duration = track.duration; // Option<u64> seconds
        if let Err(e) = self.player.play(&track.path, idx, duration, resume) {
            self.message = Some(format!("Playback error: {}", e));
        }
    }

    pub fn toggle_pause(&self) {
        self.player.toggle_pause();
    }

    pub fn stop(&mut self) {
        let idx = self.player.status.lock().unwrap().track_index;
        if let Some(i) = idx {
            let path = self.tracks[i].path.clone();
            self.last_played_path = Some(path.clone());
            self.player.stop_and_remember(&path);
        } else {
            self.player.stop();
        }
        self.message = None;
    }

    pub fn play_next_in_queue(&mut self) {
        if self.queue.is_empty() { return; }
        let next_idx = self.queue.remove(0);
        if self.queue_selected > 0 && self.queue_selected >= self.queue.len() {
            self.queue_selected = self.queue.len().saturating_sub(1);
        }
        let track = &self.tracks[next_idx];
        let resume = self.player.take_resume(&track.path);
        let duration = track.duration;
        if let Err(e) = self.player.play(&track.path, next_idx, duration, resume) {
            self.message = Some(format!("Playback error: {}", e));
        }
    }

    pub fn check_auto_advance(&mut self) {
        let current = self.player.status.lock().unwrap().state.clone();
        if self.prev_player_state == PlaybackState::Playing
            && current == PlaybackState::Stopped
        {
            self.play_next_in_queue();
        }
        self.prev_player_state = current;
    }

    // ── Queue management ──────────────────────────────────────────────────

    pub fn queue_add(&mut self) {
        if self.tracks.is_empty() { return; }
        let idx = self.selected;
        if !self.queue.contains(&idx) {
            self.queue.push(idx);
        }
    }

    pub fn queue_add_all(&mut self) {
        for i in 0..self.tracks.len() {
            if !self.queue.contains(&i) {
                self.queue.push(i);
            }
        }
    }

    pub fn queue_remove_selected(&mut self) {
        if self.focus != Panel::Queue || self.queue.is_empty() { return; }
        self.queue.remove(self.queue_selected);
        if self.queue_selected > 0 && self.queue_selected >= self.queue.len() {
            self.queue_selected = self.queue.len().saturating_sub(1);
        }
    }

    pub fn queue_clear(&mut self) {
        self.queue.clear();
        self.queue_selected = 0;
    }

    // ── Playlist persistence ──────────────────────────────────────────────

    pub fn playlist_save(&mut self, name: &str) {
        if self.queue.is_empty() {
            self.message = Some("Queue is empty — nothing to save.".into());
            return;
        }
        let tracks: Vec<String> = self
            .queue
            .iter()
            .map(|&i| self.tracks[i].path.clone())
            .collect();
        let pl = PlaylistFile {
            name: name.to_string(),
            tracks,
        };
        let path = self.music_dir.join(format!("{}.playlist.json", sanitise(name)));
        match serde_json::to_string_pretty(&pl) {
            Ok(json) => {
                if let Err(e) = fs::write(&path, json) {
                    self.message = Some(format!("Failed to save playlist: {}", e));
                } else {
                    self.message = Some(format!("Saved playlist \"{}\"", name));
                }
            }
            Err(e) => self.message = Some(format!("JSON error: {}", e)),
        }
    }

    pub fn playlist_load(&mut self, name: &str) {
        let path = self.music_dir.join(format!("{}.playlist.json", sanitise(name)));
        let json = match fs::read_to_string(&path) {
            Ok(j) => j,
            Err(e) => {
                self.message = Some(format!("Could not read playlist: {}", e));
                return;
            }
        };
        let pl: PlaylistFile = match serde_json::from_str(&json) {
            Ok(p) => p,
            Err(e) => {
                self.message = Some(format!("Bad playlist file: {}", e));
                return;
            }
        };
        // Match file paths to library indices.
        let mut found = 0usize;
        for p in &pl.tracks {
            if let Some(idx) = self.tracks.iter().position(|t| &t.path == p) {
                if !self.queue.contains(&idx) {
                    self.queue.push(idx);
                    found += 1;
                }
            }
        }
        self.message = Some(format!(
            "Loaded \"{}\" — {} / {} tracks matched.",
            pl.name,
            found,
            pl.tracks.len()
        ));
    }

    /// List available playlist names (without extension) in the music dir.
    pub fn list_playlists(&self) -> Vec<String> {
        let mut names = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.music_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.ends_with(".playlist.json") {
                    names.push(name.strip_suffix(".playlist.json").unwrap_or("").to_string());
                }
            }
        }
        names.sort();
        names
    }

    // ── Prompt ───────────────────────────────────────────────────────────

    pub fn open_prompt(&mut self, action: PromptAction, default: &str) {
        self.prompt_action = action;
        self.prompt = Some(Prompt::new(
            match action {
                PromptAction::SavePlaylist => "Save playlist as",
                PromptAction::LoadPlaylist => "Load playlist",
                PromptAction::CdExport => "CD export name",
                PromptAction::CopyToFolder => "Copy to folder",
            },
            default,
        ));
        self.message = None;
    }

    pub fn confirm_prompt(&mut self) {
        let value = match self.prompt.as_ref() {
            Some(p) => p.value().to_string(),
            None => return,
        };
        let action = self.prompt_action;
        self.prompt = None;

        if value.is_empty() {
            self.message = Some("Cancelled — empty name.".into());
            return;
        }

        match action {
            PromptAction::SavePlaylist => self.playlist_save(&value),
            PromptAction::LoadPlaylist => self.playlist_load(&value),
            PromptAction::CdExport => self.cd_export(&value),
            PromptAction::CopyToFolder => self.copy_tracks_to(&value),
        }
    }

    pub fn cancel_prompt(&mut self) {
        self.prompt = None;
    }

    // ── Burn to CD ────────────────────────────────────────────────────────

    pub fn burn_last_export(&mut self, device: Option<&str>) {
        // Find the most recent cd_* folder.
        let prefix = "cd_";
        let mut dirs: Vec<PathBuf> = match fs::read_dir(&self.music_dir) {
            Ok(entries) => entries
                .flatten()
                .filter_map(|e| {
                    let p = e.path();
                    if p.is_dir()
                        && p.file_name()
                            .and_then(|n| n.to_str())
                            .map(|n| n.starts_with(prefix))
                            .unwrap_or(false)
                    {
                        Some(p)
                    } else {
                        None
                    }
                })
                .collect(),
            Err(_) => {
                self.message = Some("Cannot read music directory.".into());
                return;
            }
        };

        if dirs.is_empty() {
            self.message =
                Some("No CD export found.  Use 'x' to export a CD image first.".into());
            return;
        }

        // Pick the most-recently-modified folder.
        dirs.sort_by_key(|d| {
            d.metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
        });
        let latest = dirs.last().unwrap();

        self.message = Some(format!("Burning {} to disc…", latest.display()));

        match crate::cd_writer::burn_disc(latest, device) {
            Ok(msg) => self.message = Some(msg),
            Err(e) => self.message = Some(format!("Burn failed: {}", e)),
        }
    }

    // ── Copy to folder ────────────────────────────────────────────────────

    pub fn copy_tracks_to(&mut self, dest: &str) {
        if self.queue.is_empty() {
            self.message = Some("Queue is empty — nothing to copy.".into());
            return;
        }
        let tracks: Vec<(String, String)> = self
            .queue
            .iter()
            .map(|&i| {
                let t = &self.tracks[i];
                (t.path.clone(), format!("{} - {}", t.artist, t.title))
            })
            .collect();

        let dest_path = PathBuf::from(dest);
        match crate::cd_writer::copy_tracks_to_folder(&tracks, &dest_path) {
            Ok((copied, total)) => {
                self.message = Some(format!(
                    "Copied {}/{} tracks to {}",
                    copied, total, dest_path.display()
                ));
            }
            Err(e) => {
                self.message = Some(format!("Copy failed: {}", e));
            }
        }
    }

    // ── CD export ─────────────────────────────────────────────────────────

    pub fn cd_export(&mut self, name: &str) {
        if self.queue.is_empty() {
            self.message = Some("Queue is empty — nothing to export.".into());
            return;
        }
        let tracks: Vec<(String, String)> = self
            .queue
            .iter()
            .map(|&i| {
                let t = &self.tracks[i];
                (t.path.clone(), format!("{} - {}", t.artist, t.title))
            })
            .collect();

        self.message = Some(format!(
            "Exporting {} tracks to CD image (44.1 kHz stereo)…",
            tracks.len()
        ));

        // Run synchronously — short playlists are fine; large ones will
        // block the UI briefly.  For very large queues call from a thread.
        match crate::cd_writer::export_playlist(&tracks, name, &self.music_dir) {
            Ok(dir) => {
                self.message = Some(format!(
                    "CD image written to {} — burn disc.cue with cdrdao or cdrecord",
                    dir.display()
                ));
            }
            Err(e) => {
                self.message = Some(format!("CD export failed: {}", e));
            }
        }
    }

    // ── Metadata editing ──────────────────────────────────────────────────

    pub fn open_editor(&mut self) {
        let idx = match self.focus {
            Panel::Library => self.selected,
            Panel::Queue => self.queue.get(self.queue_selected).copied().unwrap_or(self.selected),
        };
        if self.tracks.is_empty() { return; }
        let track = &self.tracks[idx];
        self.editor = Some(MetadataEditor::from_track(track, idx));
        self.message = None;
    }

    pub fn save_metadata(&mut self) {
        let editor = match self.editor.take() {
            Some(e) => e,
            None => return,
        };
        let path = self.tracks[editor.track_index].path.clone();
        let (title, artist, album, genre) = editor.values();

        if let Err(e) = crate::metadata::write_metadata(&path, title, artist, album, genre) {
            self.message = Some(format!("Failed to write tags: {}", e));
        }

        {
            let t = &mut self.tracks[editor.track_index];
            t.title = title.to_string();
            t.artist = artist.to_string();
            t.album = album.to_string();
            t.genre = genre.to_string();
        }

        let conn = match rusqlite::Connection::open(&self.db_path) {
            Ok(c) => c,
            Err(e) => {
                self.message = Some(format!("DB error: {}", e));
                return;
            }
        };
        if let Err(e) =
            crate::db::update_track_metadata(&conn, editor.track_id, title, artist, album, genre)
        {
            self.message = Some(format!("DB update error: {}", e));
        }
    }

    pub fn cancel_editor(&mut self) {
        self.editor = None;
    }

    // ── Time helpers for the UI ───────────────────────────────────────────

    /// Returns (elapsed_secs, total_secs) for the current track.
    pub fn playback_times(&self) -> (f64, Option<u64>) {
        let elapsed = self.player.elapsed_secs();
        let idx = self.player.status.lock().unwrap().track_index;
        let total = idx.and_then(|i| self.tracks.get(i)).and_then(|t| t.duration);
        (elapsed, total)
    }
}

/// Replace characters that are awkward in filenames.
fn sanitise(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' { c } else { '_' })
        .collect()
}
