use rusqlite::Connection;
use crate::models::Track;

pub fn load_tracks(
    conn: &Connection
) -> Vec<Track> {
    let mut stmt = conn
        .prepare(
            "
            SELECT
                id,
                path,
                title,
                artist,
                album,
                genre,
                year,
                duration
            FROM tracks
            "
        )
        .unwrap();
    stmt.query_map([], |r| {
        Ok(Track {
            id: r.get(0)?,
            path: r.get(1)?,
            title: r.get(2)?,
            artist: r.get(3)?,
            album: r.get(4)?,
            genre: r.get(5)?,
            year: r.get(6)?,
            duration: r.get(7)?,
        })
    })
    .unwrap()
    .filter_map(Result::ok)
    .collect()
}