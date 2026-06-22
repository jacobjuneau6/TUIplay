extern crate rusqlite;
extern crate lofty;
extern crate anyhow;
extern crate tokio;
extern crate ratatui;
extern crate walkdir;

mod app;
mod db;
mod library;
mod metadata;
mod models;
mod player;
mod scanner;
mod ui;

use anyhow::Result;

fn main() -> Result<()> {
    let conn = 
        db::init_db("music.db")?;
    
    scanner::scan_folder(
        &conn, "/home/jacob/Music"
    )?;

    let tracks = 
        library::load_tracks(
            &conn
        );

    println!(
        "Loaded {} tracks",
        tracks.len()
    );

    Ok(())
}