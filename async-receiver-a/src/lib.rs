use kinode_process_lib::http::server::HttpServer;
use kinode_process_lib::{kiprintln, Message};
use serde::{Deserialize, Serialize};

use kinode_app_common::erect;
use kinode_app_common::State;
use kinode_process_lib::Response;
use kinode_process_lib::http::server::{HttpBindingConfig, WsBindingConfig};
mod kino_local_handlers;
mod structs;

use kino_local_handlers::*;
use shared::*;
use structs::*;

wit_bindgen::generate!({
    path: "target/wit",
    world: "async-app-template-dot-os-v0",
    generate_unused_types: true,
    additional_derives: [serde::Deserialize, serde::Serialize, process_macros::SerdeJsonInto],
});

fn init_fn(_state: &mut AppState) {
    kiprintln!("Initializing Async Receiver A");
}

erect!(
    "Async Receiver A",
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

