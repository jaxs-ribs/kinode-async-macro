use kinode_process_lib::http::server::{HttpResponse, HttpServer};
use kinode_process_lib::{kiprintln, Message};
use serde::{Deserialize, Serialize};

use kinode_process_lib::http::server::WsMessageType;
use kinode_process_lib::http::StatusCode;
use kinode_process_lib::http::server::HttpBindingConfig;
use kinode_process_lib::http::server::WsBindingConfig;
use kinode_process_lib::LazyLoadBlob;

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

fn init_fn(_state: &mut ProcessState) {
    kiprintln!("Initializing Async Requester");
}

erect!(
    "Async Requester",
    None,
    None,
    HttpBindingConfig::default(),
    HttpBindingConfig::default(),
    WsBindingConfig::default(),
    http_handler,
    kino_local_handler,
    kino_remote_handler,
    ws_handler,
    init_fn
);

// m our@async-requester:async-app:template.os '"abc"'
