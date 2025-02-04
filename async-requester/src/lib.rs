use kinode_process_lib::http::server::{HttpResponse, HttpServer};
use kinode_process_lib::{kiprintln, Message};
use serde::{Deserialize, Serialize};

use kinode_process_lib::http::server::WsMessageType;
use kinode_process_lib::http::StatusCode;
use kinode_process_lib::http::server::HttpBindingConfig;
use kinode_process_lib::http::server::WsBindingConfig;
use kinode_process_lib::LazyLoadBlob;
use kinode_app_common::fan_out;
use kinode_process_lib::Address;

use kinode_app_common::erect;
use kinode_app_common::State;
use shared::receiver_address_a;
use kinode_app_common::timer;

use proc_macro_send::send_async;

mod http_handlers;
mod kino_local_handlers;
mod kino_remote_handlers;
mod structs;

use http_handlers::*;
use kino_local_handlers::*;
use kino_remote_handlers::*;
use shared::*;
use structs::*;

wit_bindgen::generate!({
    path: "target/wit",
    world: "async-app-template-dot-os-v0",
    generate_unused_types: true,
    additional_derives: [serde::Deserialize, serde::Serialize, process_macros::SerdeJsonInto],
});

fn init_fn(state: &mut ProcessState) {
    kiprintln!("Initializing Async Requester");
    kiprintln!("Starting timer...");
    repeated_timer(state);

    kiprintln!("Sleeping for 4 seconds before sending fanout...");
    std::thread::sleep(std::time::Duration::from_secs(4));
    fanout_message();

}

fn fanout_message() {
    let addresses: Vec<Address> = vec![
        ("our", "async-receiver-zzz", "async-app", "uncentered.os").into(), // Doesn't exist
        receiver_address_a(),
        receiver_address_b(),
        receiver_address_c(),
        ("our", "async-receiver-zzz123", "async-app", "uncentered.os").into(), // Doesn't exist
    ];

    let requests: Vec<AsyncRequest> = vec![
        AsyncRequest::Gather(()),
        AsyncRequest::Gather(()),
        AsyncRequest::Gather(()),
        AsyncRequest::Gather(()),
        AsyncRequest::Gather(()),
    ];

    fan_out!(
        addresses,
        requests,
        (all_results, st: ProcessState) {
            kiprintln!("fan_out done => subresponses: {:#?}", all_results);
            st.counter += 1;
        },
        5
    );
}

erect!(
    "Async Requester",
    None,
    None,
    HttpBindingConfig::default(),
    HttpBindingConfig::default(),
    WsBindingConfig::default(),
    _,
    _,
    _,
    _,
    _
);

// m our@async-requester:async-app:template.os '"abc"'
