#![allow(warnings)] // TODO: Zena: Remove this and fix warnings
use hyperprocess_macro::hyperprocess;
use hyperware_app_common::State;
use hyperware_process_lib::kiprintln;
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
    fn increment_counter(
        &mut self,
        value: i32,
        another_value: String,
        yet_another_value: f32,
    ) -> String {
        self.request_count += 1;
        kiprintln!("Called with: {} {} {}", value, another_value, yet_another_value);
        kiprintln!("Counter: {}", self.request_count);
        "some string".to_string()
    }

    #[remote]
    #[local]
    fn increment_counter_2(&mut self, value: f64, another_value: Vec<String>, yet_another_value: bool) -> Vec<i32> {
        self.request_count += 1;
        kiprintln!("Called with: {} {:?} {}", value, another_value, yet_another_value);
        kiprintln!("Counter: {}", self.request_count);
        vec![42, 43, 44]
    }

    #[local]
    async fn increment_counter_async(&mut self, value: i32, name: String) -> String {
        self.request_count += 1;
        kiprintln!("Starting async operations for {}", name);
        let user_data = fetch_data("users", value).await;
        let stats_data = fetch_data("stats", value).await;
        let result = format!("{} | {}", user_data, stats_data);
        kiprintln!("Completed. Result: {}", result);
        format!("Results for {}: {}", name, result)
    }

    #[remote]
    fn some_other_function(&mut self, string_val: String, another_string_val: String) -> f32 {
        self.request_count += 1;
        kiprintln!(
            "We have been called with thes following values: {:?}, {:?}",
            string_val,
            another_string_val
        );
        0.0
    }

    #[http]
    async fn increment_counter_3(&mut self, string_val: String) -> f32 {
        self.request_count += 1;
        kiprintln!(
            "We have been called with thes following values: {:?}",
            string_val
        );
        0.0
    }
}

async fn fetch_data(endpoint: &str, id: i32) -> String {
    kiprintln!("Fetching data from {} with id {}", endpoint, id);
    // In a real app, this would make an actual HTTP request
    // For this test, we're just simulating an async operation
    format!("Data from {} for id {}", endpoint, id)
}

/*
We want to be able to handle an arbitrary number of parameters for a request.
m our@hyperdriver:async-app:uncentered.os '{"IncrementCounter": [42, "abc", 3.14]}'
m our@hyperdriver:async-app:uncentered.os '{"IncrementCounter2": [42.0, ["abc", "def"], true]}'
m our@hyperdriver:async-app:uncentered.os '{"IncrementCounterAsync": [42, "test-user"]}'

curl -X POST -H "Content-Type: application/json" -d '{"IncrementCounter3": "test-string"}' http://localhost:8080/hyperdriver:async-app:uncentered.os/api
*/
