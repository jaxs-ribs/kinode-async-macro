// src/lib.rs
use hyperprocess_macro::hyperprocess;
use hyperware_app_common::{get_path, get_server, Binding, SaveOptions, State};
use hyperware_process_lib::kiprintln;
use hyperware_process_lib::{
    http::server::{HttpBindingConfig, HttpServerRequest},
    Message,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
        let path = get_path();
        kiprintln!("Path: {:?}", path);   
        let server = get_server();
        kiprintln!("Server: {:#?}", server);
    }

    #[http]
    #[local]
    fn handle_http(&mut self, message: &Message, req: Value) {
        let path = get_path();
        kiprintln!("Received HTTP request at path: {:?}", path);
        kiprintln!("Request: {:#?}", req);
        self.request_count += 1;
        // TODO: This is not sending responses
    }

    #[local]
    #[remote]
    fn handle_local(&mut self, message: &Message, req: Value) {
        let server = get_server();
        kiprintln!("Local request received");
        kiprintln!("Message: {:#?}", message);
        kiprintln!("Server: {:#?}", server);
        kiprintln!("Request: {:#?}", req);
    }
}

/*
m our@hyperdriver:async-app:uncentered.os '"abc"'
curl -X POST -H "Content-Type: application/json" -d '{"message": "hello world"}' http://localhost:8080/hyperdriver:async-app:uncentered.os/api
*/

/*
TODO: 
- Test whether the init function prints properly and gets everything
- Test whether the ws handler works
- Declare a good combo of local, remote, and http handlers and test that they work

- Move the new functions over to the proc macro
- Move as much from the common to a utils.rs file, everything that won't get `quote`d
*/