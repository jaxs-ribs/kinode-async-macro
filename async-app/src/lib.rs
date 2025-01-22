use kinode_process_lib::http::server::{HttpResponse, HttpServer};
use kinode_process_lib::{kiprintln, Address, Message};
use serde::{Deserialize, Serialize};

use kinode_process_lib::http::StatusCode;
use kinode_app_common::erect;
use kinode_app_common::State;
use shared::receiver_address;

use proc_macro_send::send_async;

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
    "My Example App",
    None,
    None,
    my_api_handler,
    my_local_request,
    my_remote_request
);

// m our@async-app:async-app:template.os '"abc"'
