use rusqlite::{Connection, Result};

pub fn init_db(path: &str) -> Result<Connection> {
    let conn = Connection::open(path)?;

    conn.execute(
        "
        CREATE TABLE IF NOT EXISTS tracks(
            id INTEGER PRIMARY KEY,
            path TEXT UNIQUE,
            title TEXT,
            artist TEXT,
            album TEXT,
            genre TEXT,
            year INTEGER,
            duration INTEGER
        )
        ",
        [],
    )?;

    Ok(conn)
}

pub fn update_track_metadata(
    conn: &Connection,
    id: i64,
    title: &str,
    artist: &str,
    album: &str,
    genre: &str,
) -> Result<()> {
    conn.execute(
        "
        UPDATE tracks
        SET title = ?1, artist = ?2, album = ?3, genre = ?4
        WHERE id = ?5
        ",
        rusqlite::params![title, artist, album, genre, id],
    )?;
    Ok(())
}
