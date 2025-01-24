use crate::*;

#[derive(Debug, Serialize, Deserialize)]
pub struct AppState {
    pub counter: u64,
}

impl State for AppState {
    fn new() -> Self {
        Self { counter: 0 }
    }
}
