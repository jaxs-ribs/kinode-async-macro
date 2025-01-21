use kinode_process_lib::http::server::{HttpResponse, HttpServer};
use kinode_process_lib::{kiprintln, Address, Message};
use serde::{Deserialize, Serialize};

use kinode_process_lib::http::StatusCode;
use process_macros::SerdeJsonInto;
use kinode_app_common::erect;
use kinode_app_common::State;

use proc_macro_send::send_async;

use crate::AsyncResponse;
use crate::AsyncRequest;
mod structs;

use structs::*;

pub fn receiver_address() -> Address {
    ("our", "async-receiver", "async-callbacks", "template.os").into()
}

wit_bindgen::generate!({
    path: "target/wit",
    world: "async-app-template-dot-os-v0",
    generate_unused_types: true,
    additional_derives: [serde::Deserialize, serde::Serialize, process_macros::SerdeJsonInto],
});

fn my_api_handler(_state: &mut AppState, _payload: String) -> (HttpResponse, Vec<u8>) {
    (HttpResponse::new(StatusCode::OK), "".as_bytes().to_vec())
}

fn my_remote_request(
    _message: &Message,
    _state: &mut AppState,
    _server: &mut HttpServer,
    _request: String,
) {
    kiprintln!("Hi2");
}

fn my_local_request(
    _message: &Message,
    _state: &mut AppState,
    _server: &mut HttpServer,
    _request: String,
) {
    send_async!(
        receiver_address(),
        AsyncRequest::StepA("Yes hello".to_string()),
        (resp, st: AppState) {
            custom_handler_a(resp, st);
        },
    );

    // send_async!(
    //     receiver_address(),
    //     AsyncRequest::StepB("Yes hello".to_string()),
    //     (resp, st: AppState) {
    //         custom_handler(resp, st);
    //     },
    //     30,
    //     on_timeout => {
    //         println!("timed out!");
    //         st.counter -= 1;
    //     }
    // );
}

fn custom_handler_a(response: String, state: &mut AppState) {
    kiprintln!("{}", response);
    kiprintln!("{}", state.counter);
    state.counter += 1;
    send_async!(
        receiver_address(),
        AsyncRequest::StepB("Yes hello".to_string()),
        (resp, st: AppState) {
            custom_handler_b(resp, st);
        },
    );
}

fn custom_handler_b(response: String, state: &mut AppState) {
    kiprintln!("{}", response);
    kiprintln!("{}", state.counter);
    state.counter += 1;
    send_async!(
        receiver_address(),
        AsyncRequest::StepC("Yes hello".to_string()),
        (resp, st: AppState) {
            custom_handler_c(resp, st);
        },
    );
}

fn custom_handler_c(response: String, state: &mut AppState) {
    kiprintln!("{}", response);
    kiprintln!("{}", state.counter);
    state.counter += 1;
}

erect!(
    "My Example App",
    None,
    None,
    my_api_handler,
    my_local_request,
    my_remote_request
);

// m our@async-app:async-app:template.os '"abc"'
