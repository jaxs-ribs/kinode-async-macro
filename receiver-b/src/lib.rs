#![allow(warnings)] // TODO: Zena: Remove this and fix warnings
use hyperprocess_macro::hyperprocess;
use hyperware_app_common::State;
use hyperware_process_lib::LazyLoadBlob;
use hyperware_process_lib::{http::server::WsMessageType, kiprintln};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct ReceiverBState {
    request_count: u64,
}

impl State for ReceiverBState {
    fn new() -> Self {
        Self { request_count: 0 }
    }
}

#[hyperprocess(
    name = "Receiver B",
    ui = Some(HttpBindingConfig::default()),
    endpoints = vec![],
    save_config = SaveOptions::EveryMessage,
    wit_world = "async-app-template-dot-os-v0"
)]
impl ReceiverBState {
    #[init]
    async fn initialize(&mut self) {
        kiprintln!("Initializing Receiver B");
        self.request_count = 0;
        kiprintln!("The counter is now {}", self.request_count);
    }

    #[local]
    fn hello(&mut self, value: i32) -> String {
        return "Hello".to_string();
    }
}
