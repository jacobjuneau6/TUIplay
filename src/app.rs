use crate::models::Track;

pub struct App {
    pub tracks: Vec<Track>,
    pub selected: usize,
}

impl App {
    pub fn new(
        tracks: Vec<Track>
    ) -> Self {
        Self {
            tracks,
            selected: 0,
        }
    }
}