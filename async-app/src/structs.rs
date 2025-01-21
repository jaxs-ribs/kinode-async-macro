use crate::*;

#[derive(Debug, Serialize, Deserialize, SerdeJsonInto, Clone)]
pub enum AsyncRequest {
    StepA(String),
    StepB(String),
    StepC(String),
}

#[derive(Debug, Serialize, Deserialize, SerdeJsonInto, Clone)]
pub enum AsyncResponse {
    StepA(String),
    StepB(String),
    StepC(String),
}

#[derive(Debug, Serialize, Deserialize, SerdeJsonInto, Clone)]
pub struct TempStruct {
    pub counter: u64,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AppState {
    pub counter: u64,
}

impl State for AppState {
    fn new() -> Self {
        Self { counter: 0 }
    }
}
