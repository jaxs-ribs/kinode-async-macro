use kinode_process_lib::http::server::HttpServer;
use kinode_process_lib::{kiprintln, Message};
use serde::{Deserialize, Serialize};

use kinode_app_common::{erect, fan_out, timer, Binding, State};
use kinode_process_lib::http::server::HttpBindingConfig;
use kinode_process_lib::Address;
use proc_macro_send::send_async;
use shared::receiver_address_a;

mod helpers;
mod structs;

use helpers::*;
use shared::*;
use structs::*;

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
    name: "Async Requester",
    icon: None,
    widget: None,
    ui: Some(HttpBindingConfig::default()),
    endpoints: [
        Binding::Http {
            path: "/api",
            config: HttpBindingConfig::default(),
        },
    ],
    handlers: {
        http: _,
        local: kino_local_handler,
        remote: _,
        ws: _,
    },
    init: init_fn,
    wit_world: "async-app-template-dot-os-v0"
);

// m our@async-requester:async-app:template.os '"abc"'
