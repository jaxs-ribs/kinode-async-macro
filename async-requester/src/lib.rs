use hyperware_process_lib::kiprintln;
use serde::{Deserialize, Serialize};

use hyperware_app_common::{hyper, hyperprocess, send, Binding, SaveOptions, State};
use hyperware_app_common::{send_parallel_requests, SendResult};
use hyperware_process_lib::http::server::HttpBindingConfig;
use hyperware_process_lib::Address;
use serde_json::Value;
use shared::{AsyncRequest, AsyncResponse};

use hyperprocess_macro::hyperprocess_;

mod helpers;
mod structs;

use shared::receiver_address_a;
use structs::*;

fn sleep(secs: u64) {
    std::thread::sleep(std::time::Duration::from_secs(2));
}

fn init_fn(_state: &mut ProcessState) {
    kiprintln!("Initializing Async Requester, sleeping 2 seconds...");
    sleep(2);

    hyper!(
        sleep(0);
        kiprintln!("Sending offline request...");

        let result: SendResult<AsyncResponse> = send(
            AsyncRequest::StepA("Hello, world!".to_string()),
            ("inexistent.os", "something", "something", "uncentered.os").into(), // Doesn't exist
            5
        ).await;
        kiprintln!("Result: {:#?}", result);
    );

    hyper!(
        sleep(5);
        kiprintln!("Sending working request to A...");

        let result: SendResult<AsyncResponse> = send(
            AsyncRequest::StepA("Hello, world!".to_string()),
            receiver_address_a(),
            5
        ).await;
        match result {
            SendResult::Success(AsyncResponse::StepA(res)) => kiprintln!("Step A: {}", res),
            _ => kiprintln!("Unknown response"),
        }
    );

    hyper!(
        sleep(10);
        kiprintln!("Sending parallel requests...");

        let results: Vec<SendResult<AsyncResponse>> = send_parallel_requests(
            vec![
                ("our", "something", "something", "uncentered.os").into(), // Doesn't exist
                ("our", "async-receiver-a", "async-app", "uncentered.os").into(),
                ("our", "async-receiver-b", "async-app", "uncentered.os").into(),
                ("our", "async-receiver-c", "async-app", "uncentered.os").into(),
                ("our", "something", "something", "uncentered.os").into(), // Doesn't exist
            ],
            vec![AsyncRequest::Gather("yes hello".to_string()); 5],
            10
        ).await;

        for result in results {
            kiprintln!("Result {:#?}", result);
        }
    );
}

fn http_handler(_state: &mut ProcessState, path: &str, req: Value) {
    kiprintln!("Received HTTP request: {:#?}", req);
    kiprintln!("Path is {:#?}", path);
}

hyperprocess!(
    name: "Async Requester",
    icon: None,
    widget: None,
    ui: Some(HttpBindingConfig::default()),
    endpoints: [
        Binding::Http {
            path: "/api",
            config: HttpBindingConfig::new(false, false, false, None),
        },
    ],
    save_config: SaveOptions::EveryMessage,
    handlers: {
        http: http_handler,
        local: _,
        remote: _,
        ws: _,
    },
    init: init_fn,
    wit_world: "async-app-template-dot-os-v0"
);

#[derive(Debug, Serialize, Deserialize)]
pub struct ProcessState {
    pub counter: u64,
}

impl State for ProcessState {
    fn new() -> Self {
        Self { counter: 0 }
    }
}