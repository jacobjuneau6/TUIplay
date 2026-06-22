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