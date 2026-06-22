#[derive(Debug, Clone)]
pub struct Track {
    pub id: i64,
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub year: Option<i32>,
    pub duration: u64,
}