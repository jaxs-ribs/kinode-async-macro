use hyperprocess_macro::hyperprocess;
use hyperware_process_lib::http::server::HttpBindingConfig;
use hyperware_process_lib::kiprintln;
use hyperware_app_common::{State, Binding};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct AsyncRequesterState {
    request_count: u64,
}

impl State for AsyncRequesterState {
    fn new() -> Self {
        Self { request_count: 0 }
    }
}

#[hyperprocess(
    name = "Async Requester",
    ui = Some(HttpBindingConfig::default()),
    endpoints = vec![
        Binding::Http {
            path: "/api",
            config: HttpBindingConfig::new(false, false, false, None),
        }
    ],
    save_config = SaveOptions::EveryMessage,
    wit_world = "async-app-template-dot-os-v0"
)]
impl AsyncRequesterState {
    #[init]
    fn initialize(&mut self) {
        kiprintln!("Initializing Async Requester");
        self.request_count = 0;
    }

    // New local handler style
    #[local]
    fn increment_counter(&mut self, value: i32) -> String {
        self.request_count += 1;
        format!("Incremented counter to {} with value {}", self.request_count, value)
    }
}