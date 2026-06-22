use std::fmt::format;

use ratatui::{
    widgets::*,
    layout::*,
    Frame,
};

use crate::app::App;

pub fn draw(
    frame: &mut Frame,
    app: &App
) {
    let chunks = 
        Layout::default()
            .direction(
                Direction::Vertical
            )
            .constraints([
                Constraint::Min(1),
            ])
            .split(frame.area());
    let items = 
                app.tracks
                    .iter()
                    .map(|t| {
                        ListItem::new(
                            format!(
                                "{} - {}",
                                t.artist,
                                t.title
                            )
                        )
                    })
                    .collect::<Vec<_>>();
    let list =
                    List::new(items)
                        .block(
                            Block::bordered()
                                .title("Library")   
                        );
    frame.render_widget(
        list,
        chunks[0]
    );
}