use hyperware_process_lib::http::server::HttpServer;
use hyperware_process_lib::{kiprintln, Message};
use serde::{Deserialize, Serialize};

use hyperware_app_common::{hyperprocess, Binding, State, SaveOptions};
use hyperware_process_lib::http::server::HttpBindingConfig;
use shared::{AsyncRequest, AsyncResponse};
use hyperware_process_lib::Response;

mod kino_local_handlers;
mod structs;

use kino_local_handlers::*;
use structs::*;

fn init_fn(_state: &mut AppState) {
}

hyperprocess!(
    name: "Async Receiver C",
    icon: None,
    widget: None,
    ui: Some(HttpBindingConfig::default()),
    endpoints: [
        Binding::Http {
            path: "/api",
            config: HttpBindingConfig::default(),
        },
    ],
    save_config: SaveOptions::EveryMessage,
    handlers: {
        http: _,
        local: kino_local_handler,
        remote: _,
        ws: _,
    },
    init: init_fn,
    wit_world: "async-app-template-dot-os-v0"
);
