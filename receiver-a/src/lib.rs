#![allow(warnings)] // TODO: Zena: Remove this and fix warnings
use hyperprocess_macro::hyperprocess;
use hyperware_app_common::State;
use hyperware_process_lib::logging::info;
use hyperware_process_lib::LazyLoadBlob;
use hyperware_process_lib::http::server::WsMessageType;
use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Serialize, Deserialize)]
struct ReceiverAState {
    request_count: u64,
}

#[hyperprocess(
    name = "Receiver A",
    ui = Some(HttpBindingConfig::default()),
    endpoints = vec![],
    save_config = SaveOptions::EveryMessage,
    wit_world = "async-app-template-dot-os-v0"
)]
impl ReceiverAState {
    #[init]
    async fn initialize(&mut self) {
        info!("Initializing Receiver A");
        self.request_count = 0;

        // let request_json = json!({
        //     "hello": {
        //         "value": 42
        //     }
        // });
        info!("The counter is now {}", self.request_count);
    }

    #[local]
    fn call_me(&mut self, value: i32) -> String {
        info!("Receiver A: Received call_me request");
        std::thread::sleep(std::time::Duration::from_secs(3));
        return "Hello".to_string();
    }
}
