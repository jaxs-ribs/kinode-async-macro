use kinode_process_lib::http::server::{HttpResponse, HttpServer};
use kinode_process_lib::{kiprintln, Message};
use serde::{Deserialize, Serialize};

use kinode_process_lib::http::StatusCode;
use kinode_app_common::erect;
use kinode_process_lib::Response;
use kinode_app_common::State;

mod structs;
mod http_handlers;
mod kino_local_handlers;
mod kino_remote_handlers;

use structs::*;
use shared::*;
use http_handlers::*;
use kino_local_handlers::*;
use kino_remote_handlers::*;

wit_bindgen::generate!({
    path: "target/wit",
    world: "async-app-template-dot-os-v0",
    generate_unused_types: true,
    additional_derives: [serde::Deserialize, serde::Serialize, process_macros::SerdeJsonInto],
});

erect!(
    "Async Receiver",
    None,
    None,
    http_handler,
    kino_local_handler,
    kino_remote_handler
);

