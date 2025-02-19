// src/lib.rs
use serde::{Deserialize, Serialize};
use hyperware_app_common::{State, Binding, SaveOptions};
use hyperware_process_lib::{
    http::server::{HttpServer, HttpServerRequest, WsMessageType, HttpBindingConfig},
    LazyLoadBlob, Message,
};
use hyperware_process_lib::kiprintln;
use hyperprocess_macro::hyperprocess;

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

    #[http]
    fn handle_http(&mut self, path: &str, req: HttpServerRequest) {
        kiprintln!("Received HTTP request at path: {}", path);
        self.request_count += 1;
    }

    #[local]
    fn handle_local(&mut self, message: &Message, server: &mut HttpServer, req: ()) {
        kiprintln!("Local request received");
    }
}