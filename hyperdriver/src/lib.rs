#![allow(warnings)] // TODO: Zena: Remove this and fix warnings
use hyperprocess_macro::hyperprocess;
use hyperware_process_lib::kiprintln;
use hyperware_app_common::State;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct AsyncRequesterState {
    request_count: u64,
}

impl State for AsyncRequesterState {
    fn new() -> Self {
        Self { request_count: 0 }
    }
}

#[hyperprocess(
    name = "Async Requester",
    ui = Some(HttpBindingConfig::default()),
    endpoints = vec![
        Binding::Http {
            path: "/api",
            config: HttpBindingConfig::new(false, false, false, None),
        }
    ],
    save_config = SaveOptions::EveryMessage,
    wit_world = "async-app-template-dot-os-v0"
)]
impl AsyncRequesterState {
    #[init]
    fn initialize(&mut self) {
        kiprintln!("Initializing Async Requester");
        self.request_count = 0;
    }

    #[local]
    fn increment_counter(&mut self, value: i32, another_value: String, yet_another_value: f32) -> String {
        self.request_count += 1;
        kiprintln!("We have been called with thes following values: {:?}, {:?}, {:?}", value, another_value, yet_another_value);
        "some string".to_string()
    }

    #[remote]
    fn some_other_function(&mut self, string_val: String, another_string_val: String) -> f32 {
        self.request_count += 1;
        kiprintln!("We have been called with thes following values: {:?}, {:?}", string_val, another_string_val);
        0.0
    }

    #[http]
    fn increment_counter_3(&mut self, string_val: String) -> f32 {
        self.request_count += 1;
        kiprintln!("We have been called with thes following values: {:?}", string_val);
        0.0
    }
}

/*
We want to be able to handle an arbitrary number of parameters for a request.
m our@hyperdriver:async-app:template.os '"abc"'
m our@hyperdriver:async-app:uncentered.os '{"IncrementCounter": [42, "abc", 3.14]}'
*/