use kinode_process_lib::http::server::HttpServer;
use kinode_process_lib::{kiprintln, Message};
use serde::{Deserialize, Serialize};

use kinode_app_common::{erect, Binding, State, SaveOptions};
use kinode_process_lib::http::server::HttpBindingConfig;
use kinode_process_lib::Response;
mod kino_local_handlers;
mod structs;

use kino_local_handlers::*;
use shared::*;
use structs::*;

fn init_fn(_state: &mut AppState) {
    kiprintln!("Initializing Async Receiver B");
}

erect!(
    name: "Async Receiver B",
    icon: None,
    widget: None,
    ui: Some(HttpBindingConfig::default()),
    endpoints: [
        Binding::Http {
            path: "/api",
            config: HttpBindingConfig::default(),
        },
    ],
    save_config: SaveOptions::EveryNSeconds(10),
    handlers: {
        http: _,
        local: kino_local_handler,
        remote: _,
        ws: _,
    },
    init: init_fn,
    wit_world: "async-app-template-dot-os-v0"
);
