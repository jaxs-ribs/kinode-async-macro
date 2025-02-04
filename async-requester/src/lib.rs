use kinode_process_lib::http::server::HttpServer;
use kinode_process_lib::{kiprintln, Message};
use serde::{Deserialize, Serialize};

use kinode_process_lib::http::server::HttpBindingConfig;
use kinode_process_lib::http::server::WsBindingConfig;
use kinode_app_common::fan_out;
use kinode_process_lib::Address;
// TODO: Zena: Needs to be abstracted into the macro
use kinode_process_lib::timer::TimerAction;

use kinode_app_common::erect;
use kinode_app_common::State;
use shared::receiver_address_a;
use kinode_app_common::timer;
use proc_macro_send::send_async;

mod helpers;
mod structs;

use helpers::*;
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
    repeated_timer(state);

    std::thread::sleep(std::time::Duration::from_secs(4));
    fanout_message();
}

/// This will get triggered with a terminal request
/// For example, if you run `m our@async-requester:async-app:template.os '"abc"'`
/// Then we will message the async receiver who will sleep 3s then answer.
pub fn kino_local_handler(
    _message: &Message,
    _state: &mut ProcessState,
    _server: &mut HttpServer,
    _request: String,
) {
    message_a();
}


erect!(
    "Async Requester",
    None,
    None,
    HttpBindingConfig::default(),
    HttpBindingConfig::default(),
    WsBindingConfig::default(),
    _, // No HTTP API call
    kino_local_handler,
    _, // No remote request
    _, // No WS handler
    init_fn
);

// m our@async-requester:async-app:template.os '"abc"'
