use std::sync::{
    Arc,
    Mutex
};

pub struct Player {
    pub current_file:
        Arc<Mutex<Option<String>>>,
}

impl Player {
    pub fn new() -> Self {
        Self {
            current_file:
                Arc::new(
                    Mutex::new(None)
                ),
        }
    }

    pub fn play(
        &self,
        path: &str
    ) {
        *self
            .current_file
            .lock()
            .unwrap() = 
            Some(path.into());
        println!(
            "Playing {}",
            path
        );
        //Base Implementation, will add play functionality affter validating other tasks
    }
}