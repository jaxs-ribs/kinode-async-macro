use kinode_process_lib::http::server::{HttpResponse, HttpServer};
use kinode_process_lib::{kiprintln, Address, Message};
use serde::{Deserialize, Serialize};

use kinode_process_lib::http::StatusCode;
use process_macros::SerdeJsonInto;
use kinode_app_common::send;
use kinode_app_common::Erect;
use kinode_app_common::State;

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

fn my_api_handler(_state: &mut MyState, _payload: String) -> (HttpResponse, Vec<u8>) {
    (HttpResponse::new(StatusCode::OK), "".as_bytes().to_vec())
}

fn my_remote_request(
    _message: &Message,
    _state: &mut MyState,
    _server: &mut HttpServer,
    _request: String,
) {
    kiprintln!("Hi2");
}

fn my_local_request(
    _message: &Message,
    _state: &mut MyState,
    _server: &mut HttpServer,
    _request: String,
) {
    send!(
        receiver_address(),
        AsyncRequest::StepA("Yes hello".to_string()),
        (response, state: MyState) {
            custom_msg_handler(response, state);
        },
        30,
        on_timeout => {
            kiprintln!("Request to 'receiver_address()' timed out!");
            state.counter -= 1;
        }
    );
}

fn custom_msg_handler(response: AsyncResponse, user_st: &mut MyState) {
    if let AsyncResponse::StepA(msg) = response {
        kiprintln!("Async callback! got {}", msg);
        user_st.counter += 10;
        kiprintln!("New counter: {}", user_st.counter);
    }
}

Erect!(
    "My Example App",
    None,
    None,
    my_api_handler,
    my_local_request,
    my_remote_request
);

// m our@async-app:async-app:template.os '"abc"'
