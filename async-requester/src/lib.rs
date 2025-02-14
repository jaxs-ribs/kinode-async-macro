use kinode_process_lib::{kiprintln, Address};
use serde::{Deserialize, Serialize};

use kinode_app_common::{cronch, erect, send, Binding, SaveOptions, State};
use kinode_app_common::{send_parallel_requests, SendResult};
use kinode_process_lib::http::server::HttpBindingConfig;
use serde_json::Value;
use shared::{receiver_address_b, receiver_address_c, AsyncRequest, AsyncResponse};

mod helpers;
mod structs;

use shared::receiver_address_a;
use structs::*;

fn init_fn(_state: &mut ProcessState) {
    kiprintln!("Initializing Async Requester, sleeping 2 seconds...");
    std::thread::sleep(std::time::Duration::from_secs(2));

    // kiprintln!("--------------------------------");
    // cronch!(
    //     kiprintln!("Sending request to A");
    //     let result: SendResult<AsyncResponse> = send(
    //         AsyncRequest::StepA("Hello, world!".to_string()),
    //         receiver_address_a(),
    //         5
    //     ).await;
    //     match result {
    //         SendResult::Success(AsyncResponse::StepA(res)) => kiprintln!("Step A: {}", res),
    //         _ => kiprintln!("Unknown response"),
    //     }
    // );

    // kiprintln!("--------------------------------");
    // kiprintln!("Sending a request that will offline");
    // cronch!(
    //     kiprintln!("Sending request to non-existent app");
    //     let result: SendResult<AsyncResponse> = send(
    //         AsyncRequest::StepA("Hello, world!".to_string()),
    //         ("bigbooty.os", "something", "something", "uncentered.os").into(), // Doesn't exist
    //         5
    //     ).await;
    //     kiprintln!("Result: {:#?}", result);
    // );

    kiprintln!("--------------------------------");
    kiprintln!("Sleeping 1 more second");
    std::thread::sleep(std::time::Duration::from_secs(1));
    let addresses: Vec<Address> = vec![
        ("our", "something", "something", "uncentered.os").into(), // Doesn't exist
        receiver_address_a(),
        receiver_address_b(),
        receiver_address_c(),
        ("our", "something", "something", "uncentered.os").into(), // Doesn't exist
    ];
    let requests: Vec<AsyncRequest> = vec![
        AsyncRequest::Gather("yes hello".to_string()),
        AsyncRequest::Gather("yes hello".to_string()),
        AsyncRequest::Gather("yes hello".to_string()),
        AsyncRequest::Gather("yes hello".to_string()),
        AsyncRequest::Gather("yes hello".to_string()),
    ];

    cronch! (
        let results: Vec<SendResult<AsyncResponse>> = send_parallel_requests(addresses.clone(), requests, 5).await;
        for (i, result) in results.into_iter().enumerate()  {
            kiprintln!("Result {:#?}", result);
        }
    );
}

fn http_handler(_state: &mut ProcessState, path: &str, req: Value) {
    kiprintln!("Received HTTP request: {:#?}", req);
    kiprintln!("Path is {:#?}", path);
}
erect!(
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
        http: _,
        local: _,
        remote: _,
        ws: _,
    },
    init: init_fn,
    wit_world: "async-app-template-dot-os-v0"
);

/*
m our@async-requester:async-app:template.os '"abc"'
curl -X POST -H "Content-Type: application/json" -d '{"message": "hello world"}' http://localhost:8080/async-requester:async-app:uncentered.os/api
*/
