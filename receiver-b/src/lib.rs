#![allow(warnings)] // TODO: Zena: Remove this and fix warnings
use hyperprocess_macro::hyperprocess;
use hyperware_process_lib::logging::info;
use hyperware_process_lib::LazyLoadBlob;
use hyperware_process_lib::http::server::WsMessageType;
use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Serialize, Deserialize)]
struct ReceiverBState {
    request_count: u64,
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
        info!("Initializing Receiver B");
        self.request_count = 0;
        info!("The counter is now {}", self.request_count);
    }

    #[local]
    fn hello(&mut self, value: String) -> f32 {
        let num_chars = value.len() as f32;
        info!("Received string of length {}", num_chars);
        num_chars
    }
}
