use kinode_process_lib::http::server::HttpServer;
use kinode_process_lib::{kiprintln, Message};
use serde::{Deserialize, Serialize};

use kinode_app_common::{erect, Binding, State};
use kinode_process_lib::http::server::{HttpBindingConfig, WsBindingConfig};
use kinode_process_lib::Response;
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
    kiprintln!("Initializing Async Receiver C");
}

erect!(
    name: "Async Receiver C",
    icon: None,
    widget: None,
    ui: Some(HttpBindingConfig::default()),
    endpoints: [
        Binding::Http {
            path: "/api",
            config: HttpBindingConfig::default(),
        },
        Binding::Ws {
            path: "/updates",
            config: WsBindingConfig::default(),
        },
    ],
    handlers: {
        api: _,
        local: kino_local_handler,
        remote: _,
        ws: _,
    },
    init: init_fn
);
